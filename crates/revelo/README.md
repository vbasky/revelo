# revelo

Read technical metadata from any media file — pure Rust, no system dependencies.

```rust
let meta = revelo::Metadata::from_file("photo.jpg").unwrap();
for (key, value) in meta.exif() {
    println!("{key} = {value}");
}
```

See the [project README](https://github.com/vbasky/revelo#readme) for full documentation.
