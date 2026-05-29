use revelo_core::StreamCollection;
pub fn to_niso(_streams: &StreamCollection, _file_path: &str) -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<niso:metadata xmlns:niso=\"http://www.niso.org/schemas/z39.87/metadata\">\n</niso:metadata>\n".to_string()
}
