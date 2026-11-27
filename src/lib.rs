//! Spock Assistant: unofficial Spock Framework support for OpenIDEA.
//!
//! Step 1: run a Maven project's Spock specifications and show the results in the test tree.
//! The console of Surefire names test classes only, so per-feature results are read from the
//! XML report of each class once the class is done (see `surefire`).

mod blocks;
mod modules;
mod spec;
mod surefire;

use modules::{ROOT_ID, join, module_id, module_of, parse_modules};
use openidea_plugin::diagnostics::DiagnosticsProvider;
use openidea_plugin::language_features::{Document, GutterAction, LanguageFeatures};
use openidea_plugin::{
    Diagnostic, Error, Invocation, Location, Plugin, PluginRunConfiguration, PluginTestEvent, Position, Range, RunConfigurations,
    Severity, TestFailed, TestPassed, TestResults, TestSkipped, TestStart, host, register_plugin,
};
use surefire::{Case, Line, Outcome, classify, filter_args, parse_cases, select_args, simple_name};

/// How deep nested `<modules>` are followed.
const MAX_MODULE_DEPTH: usize = 4;

#[derive(Default)]
pub struct Main {
    /// Project root given to `configurations`, to find the project again while parsing.
    root: Option<String>,
    /// Console output of the run that is not yet a whole line.
    pending: String,
}

impl Plugin for Main {
    fn run_command(&mut self, id: &str, _ctx: &Invocation, arg: Option<&str>) -> Result<Option<String>, Error> {
        match id {
            "run-tests" => {
                let args: Vec<String> = arg.map(|a| vec![a.to_owned()]).unwrap_or_default();
                host::start_run(ROOT_ID, &args, false).map(Some)
            }
            // Gutter icon: argument is `<config id>\t<class>[\t<feature>]`.
            "run-spec" => {
                let mut parts = arg.unwrap_or_default().split('\t');
                let (Some(config), Some(class)) = (parts.next(), parts.next()) else {
                    return Err("run-spec needs a configuration and a class".into());
                };
                let mut path = vec![class.to_owned()];
                path.extend(parts.next().map(str::to_owned));
                let args = filter_args(&[path]).ok_or("nothing to run")?;
                host::start_run(config, &args, false).map(Some)
            }
            _ => Err(Error::NotFound(id.into())),
        }
    }
}

impl DiagnosticsProvider for Main {
    fn diagnostics(&mut self, doc: &openidea_plugin::diagnostics::Document) -> Vec<Diagnostic> {
        if !doc.path.ends_with(".groovy") {
            return vec![];
        }
        blocks::check(&doc.text)
            .into_iter()
            .map(|p| {
                let range = Range::new(Position::new(p.line, p.start), Position::new(p.line, p.end));
                let mut d = Diagnostic::new(range, Severity::Error, p.message);
                d.source = Some("Spock".into());
                d.code = Some("block-order".into());
                d
            })
            .collect()
    }
}

impl LanguageFeatures for Main {
    fn gutter_actions(&mut self, doc: &Document) -> Vec<GutterAction> {
        if !doc.path.ends_with(".groovy") {
            return vec![];
        }
        let markers = spec::find_markers(&doc.text);
        if markers.is_empty() {
            return vec![];
        }
        let config = self.config_for_file(&doc.path);
        markers
            .into_iter()
            .map(|m| {
                let (tooltip, argument) = match &m.feature {
                    Some(f) => (format!("Run '{f}'"), format!("{config}\t{}\t{f}", m.class)),
                    None => (format!("Run {}", m.class), format!("{config}\t{}", m.class)),
                };
                GutterAction {
                    line: m.line,
                    icon: Some("run".into()),
                    tooltip,
                    command: "run-spec".into(),
                    argument: Some(argument),
                }
            })
            .collect()
    }
}

impl RunConfigurations for Main {
    fn configurations(&mut self, project_root: &str) -> Vec<PluginRunConfiguration> {
        self.root = Some(project_root.to_owned());
        // Offered for every Maven project (the manifest asks for a pom.xml at the root). Spock may be
        // declared in a module's pom rather than the root one, so the root pom is not a reliable filter.
        let root = project_root.trim_end_matches(['/', '\\']);
        let mut configs = vec![maven_test(ROOT_ID, "Spock specs: whole project (mvn test)", None)];
        let mut modules = Vec::new();
        if let Some(project) = self.project() {
            collect_modules(&project, "", 0, &mut modules);
        }
        for rel in modules {
            let Some(id) = module_id(&rel) else { continue };
            let cwd = format!("{root}/{rel}");
            configs.push(maven_test(&id, &format!("Spock specs: {rel} (mvn test)"), Some(cwd)));
        }
        configs
    }
}

fn maven_test(id: &str, name: &str, cwd: Option<String>) -> PluginRunConfiguration {
    PluginRunConfiguration {
        id: id.into(),
        name: name.into(),
        program: "mvn".into(),
        args: vec!["-B".into(), "test".into()],
        cwd,
        test: true,
        framework: Some("plugin".into()),
    }
}

/// Module paths (relative to the project root) found by following `<modules>` from the pom of `parent`.
fn collect_modules(project: &host::Project, parent: &str, depth: usize, out: &mut Vec<String>) {
    if depth >= MAX_MODULE_DEPTH {
        return;
    }
    let pom = join(parent, "pom.xml");
    let Ok(text) = project.read_file(&pom) else { return };
    for child in parse_modules(&text) {
        let rel = join(parent, &child);
        if !out.contains(&rel) {
            out.push(rel.clone());
            collect_modules(project, &rel, depth + 1, out);
        }
    }
}

impl Main {
    /// Run configuration of the Maven module that owns `file`: the nearest folder above it with a
    /// `pom.xml`, below the project root. Falls back to the whole-project run.
    fn config_for_file(&self, file: &str) -> String {
        let Some(project) = self.project() else { return ROOT_ID.to_owned() };
        let Ok(root) = project.root() else { return ROOT_ID.to_owned() };
        let root = root.replace('\\', "/");
        let root = root.trim_end_matches('/');
        let file = file.replace('\\', "/");
        let head = file.get(..root.len()).unwrap_or("");
        if !head.eq_ignore_ascii_case(root) {
            return ROOT_ID.to_owned();
        }
        let mut dir = file[root.len()..].trim_matches('/').to_owned();
        while let Some(i) = dir.rfind('/') {
            dir.truncate(i);
            if project.read_file(&join(&dir, "pom.xml")).is_ok() {
                return module_id(&dir).unwrap_or_else(|| ROOT_ID.to_owned());
            }
        }
        ROOT_ID.to_owned()
    }

    fn project(&self) -> Option<host::Project> {
        let projects = host::projects().ok()?;
        let wanted = self.root.as_deref();
        let mut first = None;
        for p in projects {
            if wanted.is_some() && p.root().ok().as_deref() == wanted {
                return Some(p);
            }
            first.get_or_insert(p);
        }
        first
    }

    /// Location of a spec class in the usual Groovy test source folders, when the file exists.
    fn spec_location(&self, base: &str, fqcn: &str) -> Option<Location> {
        let project = self.project()?;
        let outer = fqcn.split('$').next().unwrap_or(fqcn);
        let rel = join(base, &format!("src/test/groovy/{}.groovy", outer.replace('.', "/")));
        project.read_file(&rel).ok()?;
        let root = project.root().ok()?;
        Some(Location {
            path: format!("{}/{rel}", root.trim_end_matches(['/', '\\'])),
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        })
    }

    fn class_events(&self, base: &str, fqcn: &str, run: u32, failed: u32, skipped: u32) -> Vec<PluginTestEvent> {
        let report = join(base, &format!("target/surefire-reports/TEST-{fqcn}.xml"));
        let cases = self.project().and_then(|p| p.read_file(&report).ok()).map(|x| parse_cases(&x));
        match cases {
            Some(cases) if !cases.is_empty() => events_for_cases(fqcn, &cases),
            // No usable report (multi-module layout, custom reports directory): keep the totals visible.
            _ => events_for_totals(fqcn, run, failed, skipped),
        }
    }

    fn line_events(&self, base: &str, line: &str) -> Vec<PluginTestEvent> {
        match classify(line) {
            Line::Running(fqcn) => vec![PluginTestEvent::SuiteStarted(TestStart {
                id: fqcn.clone(),
                parent: None,
                name: simple_name(&fqcn).to_owned(),
                location: self.spec_location(base, &fqcn),
            })],
            Line::ClassDone { fqcn, failed, skipped, run } => self.class_events(base, &fqcn, run, failed, skipped),
            Line::Other => vec![],
        }
    }
}

fn events_for_cases(fqcn: &str, cases: &[Case]) -> Vec<PluginTestEvent> {
    let mut events = Vec::with_capacity(cases.len() * 2);
    let mut seen: Vec<&str> = Vec::new();
    for case in cases {
        // A feature name can repeat (unrolled iterations): the id stays unique, the shown name stays as is.
        let n = seen.iter().filter(|s| **s == case.name).count();
        seen.push(&case.name);
        let id = if n == 0 { format!("{fqcn}#{}", case.name) } else { format!("{fqcn}#{}#{}", case.name, n + 1) };
        events.push(PluginTestEvent::TestStarted(TestStart {
            id: id.clone(),
            parent: Some(fqcn.to_owned()),
            name: case.name.clone(),
            location: None,
        }));
        events.push(match &case.outcome {
            Outcome::Passed => PluginTestEvent::Passed(TestPassed { id, duration_ms: case.time_ms }),
            Outcome::Skipped(reason) => PluginTestEvent::Skipped(TestSkipped { id, reason: reason.clone() }),
            Outcome::Failed(message) => PluginTestEvent::Failed(TestFailed {
                id,
                duration_ms: case.time_ms,
                message: message.clone(),
                expected: None,
                actual: None,
                location: None,
            }),
        });
    }
    events
}

fn events_for_totals(fqcn: &str, run: u32, failed: u32, skipped: u32) -> Vec<PluginTestEvent> {
    let id = format!("{fqcn}#(summary)");
    let name = format!("{run} run, {failed} failed, {skipped} skipped (no XML report found)");
    let mut events = vec![PluginTestEvent::TestStarted(TestStart {
        id: id.clone(),
        parent: Some(fqcn.to_owned()),
        name,
        location: None,
    })];
    events.push(if failed > 0 {
        PluginTestEvent::Failed(TestFailed {
            id,
            duration_ms: None,
            message: format!("{failed} test(s) failed in {fqcn}; see the run output"),
            expected: None,
            actual: None,
            location: None,
        })
    } else {
        PluginTestEvent::Passed(TestPassed { id, duration_ms: None })
    });
    events
}

impl TestResults for Main {
    fn select_tests(&mut self, config: &str, tests: &[Vec<String>]) -> Option<Vec<String>> {
        module_of(config)?;
        select_args(tests)
    }

    fn parse(&mut self, config: &str, chunk: &str) -> Vec<PluginTestEvent> {
        let base = module_of(config).unwrap_or_default();
        self.pending.push_str(chunk);
        let mut events = vec![];
        while let Some(end) = self.pending.find('\n') {
            let line: String = self.pending.drain(..=end).collect();
            events.extend(self.line_events(&base, &line));
        }
        if self.pending.len() > 64 * 1024 {
            self.pending.clear();
        }
        events
    }
}

register_plugin!(Main, run_configurations, test_results, language_features, diagnostics);

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const REPORT: &str = r#"<testsuite name="org.acme.FooSpec">
  <testcase name="adds two numbers" classname="FooSpec" time="0.5"/>
  <testcase name="fails" classname="FooSpec" time="0.1"><failure message="boom" type="E">trace</failure></testcase>
  <testcase name="adds two numbers" classname="FooSpec" time="0.2"/>
</testsuite>"#;

    fn host() -> MockHost {
        let mock = MockHost::install().with_capabilities(&["project.read", "process:mvn"]);
        mock.set_project_root(Some("/p"));
        mock.add_file("pom.xml", "<project><dependency>spock-core</dependency></project>");
        mock
    }

    #[test]
    fn offers_the_run_for_any_maven_project() {
        let mock = host();
        let mut plugin = Main::default();
        let configs = plugin.configurations("/p");
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].program, "mvn");
        assert_eq!(configs[0].framework.as_deref(), Some("plugin"));
        // Spock may live in a module's pom: a root pom without it still gets the run.
        mock.add_file("pom.xml", "<project/>");
        assert_eq!(plugin.configurations("/p").len(), 1);
    }

    #[test]
    fn every_module_gets_a_run_in_its_own_folder_and_reads_its_own_reports() {
        let mock = host();
        mock.add_file("pom.xml", "<project><modules><module>core</module><module>web</module></modules></project>");
        mock.add_file("core/pom.xml", "<project><modules><module>sub</module></modules></project>");
        mock.add_file("core/sub/pom.xml", "<project/>");
        mock.add_file("web/pom.xml", "<project/>");
        mock.add_file("core/src/test/groovy/org/acme/FooSpec.groovy", "class FooSpec {}");
        mock.add_file("core/target/surefire-reports/TEST-org.acme.FooSpec.xml", REPORT);
        let mut plugin = Main::default();
        let configs = plugin.configurations("/p");
        let ids: Vec<_> = configs.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["mvn-test", "mvn-test.core", "mvn-test.core.sub", "mvn-test.web"]);
        assert_eq!(configs[0].cwd, None);
        assert_eq!(configs[1].cwd.as_deref(), Some("/p/core"));
        let events = plugin.parse(
            "mvn-test.core",
            "[INFO] Running org.acme.FooSpec\n[INFO] Tests run: 3, Failures: 1, Errors: 0, Skipped: 0 -- in org.acme.FooSpec\n",
        );
        assert_eq!(events.len(), 7, "suite + 3 x (start, result)");
        let PluginTestEvent::SuiteStarted(s) = &events[0] else { panic!() };
        assert_eq!(s.location.as_ref().unwrap().path, "/p/core/src/test/groovy/org/acme/FooSpec.groovy");
        let mut plugin = Main::default();
        assert!(plugin.select_tests("mvn-test.core", &[vec!["FooSpec".into()]]).is_some());
    }

    #[test]
    fn console_output_split_anywhere_builds_the_tree_from_the_xml_report() {
        let mock = host();
        mock.add_file("src/test/groovy/org/acme/FooSpec.groovy", "class FooSpec {}");
        mock.add_file("target/surefire-reports/TEST-org.acme.FooSpec.xml", REPORT);
        let mut plugin = Main::default();
        plugin.configurations("/p");
        assert!(plugin.parse(ROOT_ID, "[INFO] Runn").is_empty());
        let mut events = plugin.parse(
            ROOT_ID,
            "ing org.acme.FooSpec\n[INFO] Tests run: 3, Failures: 1, Errors: 0, Skipped: 0, Time elapsed: 1 s <<< FAILURE! -- in org.acme.FooSpec\r\n",
        );
        let PluginTestEvent::SuiteStarted(suite) = events.remove(0) else { panic!("suite first") };
        assert_eq!((suite.id.as_str(), suite.name.as_str()), ("org.acme.FooSpec", "FooSpec"));
        assert_eq!(suite.location.unwrap().path, "/p/src/test/groovy/org/acme/FooSpec.groovy");
        let kinds: Vec<&str> = events
            .iter()
            .map(|e| match e {
                PluginTestEvent::TestStarted(_) => "start",
                PluginTestEvent::Passed(_) => "pass",
                PluginTestEvent::Failed(_) => "fail",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["start", "pass", "start", "fail", "start", "pass"]);
        let PluginTestEvent::TestStarted(dup) = &events[4] else { panic!() };
        assert_eq!(dup.name, "adds two numbers");
        assert_eq!(dup.id, "org.acme.FooSpec#adds two numbers#2");
        let PluginTestEvent::Failed(f) = &events[3] else { panic!() };
        assert_eq!((f.message.as_str(), f.duration_ms), ("boom", Some(100)));
    }

    #[test]
    fn without_a_report_the_class_totals_are_still_shown() {
        let _mock = host();
        let mut plugin = Main::default();
        let events =
            plugin.parse(ROOT_ID, "[ERROR] Tests run: 2, Failures: 1, Errors: 0, Skipped: 0 -- in org.acme.BarSpec\n");
        assert!(matches!(&events[..], [PluginTestEvent::TestStarted(_), PluginTestEvent::Failed(_)]));
    }

    #[test]
    fn selection_only_applies_to_this_plugins_run() {
        let mut plugin = Main::default();
        let t = [vec!["FooSpec".to_owned(), "fails".to_owned()]];
        assert!(plugin.select_tests(ROOT_ID, &t).is_some());
        assert_eq!(plugin.select_tests("other", &t), None);
    }

    #[test]
    fn gutter_icons_run_the_class_or_one_feature_in_the_owning_module() {
        let mock = host();
        mock.add_file("core/pom.xml", "<project/>");
        let mut plugin = Main::default();
        let doc = Document {
            path: "/p/core/src/test/groovy/org/acme/FooSpec.groovy".into(),
            text: "class FooSpec extends Specification {
  def \"adds two\"() {}
}
".into(),
            ..Default::default()
        };
        let actions = plugin.gutter_actions(&doc);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].line, 0);
        assert_eq!(actions[0].argument.as_deref(), Some("mvn-test.core	FooSpec"));
        assert_eq!(actions[1].argument.as_deref(), Some("mvn-test.core	FooSpec	adds two"));
        assert_eq!(actions[1].command, "run-spec");
        let other = Document { path: "/p/readme.md".into(), ..Default::default() };
        assert!(plugin.gutter_actions(&other).is_empty());
        let top = Document { path: "/p/src/test/groovy/BarSpec.groovy".into(), text: "class BarSpec extends Specification {}".into(), ..Default::default() };
        assert_eq!(plugin.gutter_actions(&top)[0].argument.as_deref(), Some("mvn-test	BarSpec"));
    }
}
