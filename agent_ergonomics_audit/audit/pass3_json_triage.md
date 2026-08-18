# Pass 3 — parseability + mega-command

`--json` now works on: doctor, architecture, provider list, explain, inspect, diff, fork status.
`emath agent triage <file>` returns one `emath.agent` envelope with doctor + admission + plan counts.

Probed: doctor.ok, architecture.schema, provider-list 10, explain goals, triage admitted+doctor_ok+goals=1.
Unknown provider `native.rus` suggests `emath provider inspect native.rust`.
