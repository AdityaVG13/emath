# Feature Classification Decision Tree

Given a concept from a wave, paper, user, or agent:

```text
Does it change only spelling/presentation?
    → syntax-pack, symbol, binder, or lens

Does it define a top-level declaration shape?
    → kind

Does it define a named payload inside a kind?
    → section

Does it define admissible values/representation?
    → type + constructors

Does it define abstract operations/laws?
    → theory

Does it realize a theory on a carrier?
    → instance

Does it define one operation/relation/goal?
    → capability

Does it generate repeated capabilities?
    → family

Does it interpret under-specified semantics?
    → world

Does it solve a goal?
    → method

Does it call an external engine?
    → provider

Does it define a durable output?
    → artifact

Does it define a typed failure/route?
    → diagnostic

Does it bundle features?
    → field pack

Does it evolve source or meaning?
    → migration

Is it only an idea/name with no semantics?
    → catalog entry; do not promote
```

A feature may depend on several classes but has one primary class.
