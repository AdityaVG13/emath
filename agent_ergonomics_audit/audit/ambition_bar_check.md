# Ambition bar (emath CLI, this loop)

Mode was full. Landed changes (each has a unit test or a probed transcript):

1. `emath version|--version|-V`
2. `emath help <command>`
3. `emath <command> --help`
4. command typos (`chek` → `emath check`)
5. usage errors name `emath help <command>`
6. `emath agent build` defaults `--out` to `target/emath`
7. `emath capabilities [--json]`
8. `emath robot-docs [guide]`
9. `--json` on doctor
10. `--json` on architecture
11. `--json` on explain
12. `--json` on inspect
13. `--json` on diff
14. `--json` on provider list
15. `--json` on fork status
16. `emath agent triage` mega-command
17. provider id typos (`native.rus` → `native.rust`)
18. unknown flags (`--jason` → `--json`)

Dimensions touched: first-try / intuitiveness, error pedagogy, intent inference,
self-documentation, output parseability, regression resistance.

Polish leftovers held: TTY color (CLI never emits ANSI); `NO_COLOR` is a no-op
because there is no color. `SOURCE_DATE_EPOCH` unused (no timestamps on these
surfaces). Genesis `world show` / `portfolio show` stay file-shaped reads.

No remaining first-try fail on the agent-facing catalog that scores ≥ the
apply bar without becoming new feature work.
