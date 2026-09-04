# Target Repository Tree

```text
language/
├── README.md
├── authority.lock
├── spec-holes.json
├── spec/
│   ├── meta/
│   ├── core/
│   ├── kinds/
│   ├── sections/
│   ├── symbols/
│   ├── binders/
│   ├── types/
│   ├── constructors/
│   ├── capabilities/
│   ├── theories/
│   ├── instances/
│   ├── families/
│   ├── methods/
│   ├── worlds/
│   ├── providers/
│   ├── artifacts/
│   ├── diagnostics/
│   ├── migrations/
│   ├── field-packs/
│   └── lenses/
├── conformance/
├── generated/
├── reference/       # generated plus limited hand-authored guides after migration
├── examples/
├── grammar/         # generated accepted grammar + Stage-0 source
└── templates/

agent/
├── AGENT_START.json
├── task-capsules/
├── context-cache/
├── change-contracts/
├── receipts/
├── decisions/
└── negative-knowledge/

elps/
├── ELP-TEMPLATE-V2.md
└── ...

implementation/
├── CONSTITUTION.md
├── CRATE_MAP.md
└── ...
```
