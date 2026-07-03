# revelo-wasm

WebAssembly bindings for [**revelo**](https://github.com/vbasky/revelo) — parse
media metadata in the browser with no server round-trip.

Pass a file buffer from JavaScript, get MediaInfo-style JSON back. Pure Rust under
the hood; no native dependencies at runtime.

## Install

```sh
npm install revelo-wasm
```

## Quick start

```js
import init, { parse, version } from "revelo-wasm";

await init();

console.log(version()); // e.g. "0.5.2"

const input = document.querySelector('input[type="file"]');
input.addEventListener("change", async (event) => {
  const file = event.target.files[0];
  const buf = await file.arrayBuffer();
  const json = parse(new Uint8Array(buf));

  if (json === null) {
    console.log("format not recognized");
    return;
  }

  console.log(JSON.parse(json));
});
```

## API

| Export | Description |
| --- | --- |
| `init()` | Load the WASM module. Call once before `parse`. |
| `parse(data: Uint8Array)` | Parse a media file buffer. Returns a JSON string, or `null` if the format is not recognized. |
| `version()` | Return the package version string. |

## Output shape

`parse` returns JSON matching revelo's MediaInfo-style export: an array of stream
objects, each with a `kind` (`General`, `Video`, `Audio`, `Text`, `Image`,
`EXIF`) and a `fields` array of `{ name, value }` pairs.

```json
[
  {
    "kind": "General",
    "fields": [
      { "name": "Format", "value": "MPEG-4" },
      { "name": "Duration", "value": "1 min 30 s" }
    ]
  },
  {
    "kind": "Video",
    "fields": [
      { "name": "Format", "value": "AVC" },
      { "name": "Width", "value": "1920 pixels" }
    ]
  }
]
```

## Bundlers

The package is ESM (`"type": "module"`). With Vite, webpack, or Rollup, import
as shown above. The WASM binary ships inside the package; `init()` resolves it
automatically.

If your bundler strips side-effect imports, keep the default import:

```js
import init from "revelo-wasm";
await init();
```

## Building from source

Requires [wasm-pack](https://rustwasm.github.io/wasm-pack/) and the
`wasm32-unknown-unknown` target:

```sh
rustup target add wasm32-unknown-unknown
wasm-pack build crates/revelo-wasm --release
```

The build output lands in `crates/revelo-wasm/pkg/`.

## Related

- [revelo](https://github.com/vbasky/revelo) — the Rust metadata engine
- [revelo on crates.io](https://crates.io/crates/revelo) — native Rust API

## License

BSD-2-Clause. See [LICENSE](../../LICENSE).