use revelio_core::StreamCollection;
pub fn to_fims(_streams: &StreamCollection, _file_path: &str) -> String {
    "<fims:media xmlns:fims=\"http://www.fims.tv/schemas/2013\" version=\"1.0\">\n</fims:media>\n".to_string()
}
