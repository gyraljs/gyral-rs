# gyral-renderer-domain

Validated request, limit, manifest, and HTML types shared by
[`gyral-renderer`](https://crates.io/crates/gyral-renderer) and the Rust host
that calls it. The crate is pure: it has no JavaScript runtime, HTTP framework,
clock, or filesystem access.

Most applications use these types through the
[`gyral`](https://crates.io/crates/gyral) front door, which re-exports them.
Depend on this crate directly when code must build or check requests without
linking V8, such as a pure application layer or a client-only build.

```rust
use gyral_renderer_domain::{ContractError, PageId, RenderRequest};
use serde_json::json;

fn main() -> Result<(), ContractError> {
    let page = PageId::try_from("home".to_owned())?;
    let request = RenderRequest::try_new(page, json!({"name": "friend"}))?;
    assert_eq!(request.page().as_str(), "home");

    // Page identifiers are logical names, never paths.
    assert_eq!(
        PageId::try_from("../private".to_owned()),
        Err(ContractError::InvalidPage)
    );
    Ok(())
}
```

`BundleManifest` parses the build metadata that binds a server bundle, a
browser entry, and their SHA-256 digests. It accepts a supported SSR protocol
(`SUPPORTED_PROTOCOLS`, `SsrProtocol`) from a stable Gyral release at or above
that protocol's minimum (`GyralVersion`): protocol 1 from Gyral 0.3.8 or newer.
`RenderLimits` validates the render deadline, managed heap budget, and shutdown
deadline. `RenderedHtml` bounds the renderer's output.

## Compatibility

Version 0.1 is pre-1.0. Incompatible API changes increase the minor version, and
all gyral-rs crates move together. The minimum supported Rust version is 1.99.

Read the [bundle contract](https://github.com/gyraljs/gyral-rs/blob/main/docs/design/bundle-contract.md)
for the JavaScript side of these types. Licensed under the
[MIT License](https://github.com/gyraljs/gyral-rs/blob/main/LICENSE).
