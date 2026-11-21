# Spock Assistant

> Unofficial [Spock Framework](https://spockframework.org) support for
> [OpenIDEA](https://github.com/l0ng-ai/openidea). Not affiliated with, endorsed by, or
> maintained by the Spock Framework project. Licensed under the Apache License 2.0.

Brings the everyday Spock workflow of IntelliJ IDEA to OpenIDEA.

## Status: early scaffold

Only the plugin skeleton exists (id `quonn77.spock-assistant`). Planned, in this order:

1. **Run tests and see results** — run icon in the gutter on specifications and feature
   methods, run through Gradle or Maven, results in the IDE's test tree (passed / failed,
   expected vs. actual), rerun failed tests, run a single feature.
2. **Block validation** — diagnostics for block names, their order and completeness
   (`given` / `when` / `then` / `where`, missing `then`, ...).
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
