# Patched dependencies

Copies of crates from crates.io with small fixes Ferrum needs before they are
released upstream. Each change is marked with a `FERRUM PATCH` comment.

| Crate | Version | Fix |
|---|---|---|
| `servo-script` | 0.8.0 | `dom/webgl/vertexarrayobject.rs`: `Drop` for `VertexArrayObject` no longer touches buffers that the garbage collector may already have freed. Without this, closing or reloading a tab that used WebGL 2 (three.js and most 3D sites) aborted the whole browser. |

To update Servo, delete the copy, bump the version in `Cargo.toml`, and only
re-apply a patch if the bug is still there.
