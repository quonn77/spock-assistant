# Spock Assistant

> Unofficial [Spock Framework](https://spockframework.org) support for
> [OpenIDEA](https://github.com/l0ng-ai/openidea). Not affiliated with, endorsed by, or
> maintained by the Spock Framework project. Licensed under the Apache License 2.0.

Brings the everyday Spock workflow of IntelliJ IDEA to OpenIDEA.

## Status: early, step 1 in progress

Plugin id `quonn77.spock-assistant`. Planned, in this order:

1. **Run tests and see results** — *first brick done, not yet verified inside OpenIDEA.*
   - Done (unit-tested, and the Maven filter checked on a real Spock project): a `Spock specs (mvn test)`
     run configuration for the whole project plus one per Maven module (own working directory, own reports); results in the test tree per spec and per
     feature (passed / failed / skipped, duration, failure message); rerun failed tests and run a
     single feature (`-Dtest=Class#feature`).
   - How: Surefire's console only names test classes, so the per-feature results are read from the
     `target/surefire-reports/TEST-<class>.xml` of each class when it is done. Needs `process:mvn` and `project.read`.
   - Gutter run icons on spec classes and feature methods (run the class or one feature in its module). Not yet: Gradle, the whole-project run in multi-module trees shows totals per class only (reports are per module).
   - Debug: from the test tree (right-click a spec or a feature, Debug) after a run; it reuses the
     same `-Dtest=` filter with OpenIDEA's Java debugger. Debugging the whole module run configuration
     from the run selector works too. No Debug icon in the gutter: a plugin can only debug an
     existing run configuration, so it could not filter by class or feature.
2. **Block validation** — *first part done, not yet verified inside OpenIDEA.*
   - Done: errors while typing on misplaced block labels in Groovy specs: `then` without a
     `when`, `when` without a `then`, `given`/`setup` not first, `cleanup` followed by anything but
     `where`, blocks after `where`. `and:` continues the block before it; labels inside closures,
     strings and comments are ignored.
   - Not yet: empty blocks, quick fixes, unknown labels.
3. **Specification templates** — "New Specification" and create-test for Groovy, Java and Kotlin classes.

## Build

Requires Rust (`wasm32-wasip2` target) and the `openidea-plugin` tool, see the
[OpenIDEA quickstart](https://github.com/l0ng-ai/openidea/blob/main/docs/en/plugins/quickstart.mdx).

```sh
cargo test
openidea-plugin build
openidea-plugin link     # then Settings > Plugins in OpenIDEA
```

## License

[Apache License 2.0](LICENSE). See [NOTICE](NOTICE) for attribution.

This plugin is a clean-room reimplementation for OpenIDEA's Rust/WASM plugin API, informed by
[Spock IntelliJ Plugin](https://github.com/lobodpav/spock-intellij-plugin) by Pavel Lobodinsky
(Apache-2.0, archived). Spock itself is Apache-2.0 too. Third-party names are used only to say
what this plugin works with; they belong to their owners.
