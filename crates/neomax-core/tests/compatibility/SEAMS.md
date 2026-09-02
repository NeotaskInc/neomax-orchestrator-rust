# Compatibility seams

The fixtures in this directory are sanitized snapshots of the durable JSON and JSONL contracts. They use `/workspace`, `/profiles`, and `/state` placeholders only. No provider CLI, account credential, HTTP request, or network access is needed to run these tests.

## Legacy account number strings

The history schema accepts both numeric and string `account_number` values. The field-level deserializer accepts `null`, non-negative integer-valued JSON numbers, and non-empty decimal strings that fit in `u32`. Negative, fractional, invalid, and out-of-range values are rejected. Serialization always emits the canonical numeric form.

## Malformed and missing state

Optional account controls intentionally degrade malformed or missing JSON to empty state. Durable run, task, queue, scheduler, and issue stores use stricter behavior where a malformed existing file is an error and the original bytes remain untouched. Missing directories or files produce an empty listing where the corresponding store contract supports it.
