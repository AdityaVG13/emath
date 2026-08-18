# Pass 2 — self-documentation

`emath capabilities` and `emath capabilities --json` emit the same
`emath.capabilities` document (schema, version, exit codes, command catalog).
`python json.loads` accepts the document.

`emath robot-docs` / `emath robot-docs guide` print the agent handbook.
Unknown topics (`emath robot-docs waffle`) suggest `emath robot-docs guide`.
