# V15 Realization

## Kinds

```text
cipher
puzzle
protocol
code
game
challenge
commitment
secret_sharing
zero_knowledge
stegosystem
```

Each is a Kind Capsule with sections, bounded lowering, artifacts, diagnostics, and conformance.

## Shared semantics

```text
types:
    secret, plaintext, ciphertext, key, nonce, codeword, clue, solution

theories:
    finite field, permutation, relation, hidden-state system

capabilities:
    encrypt, decrypt, encode, decode, verify, solve, generate, challenge

worlds:
    symbolic adversary, finite field, constraint, SAT/SMT/CP, measured security

methods/providers:
    exact search, solver adapters, audited production cryptography

artifacts:
    solution, certificate, protocol trace, challenge capsule, production receipt
```

## Non-negotiable

`cipher` is not an AST variant. Caesar is not a compiler builtin. Production security authority
comes from provider/evidence capsules.
