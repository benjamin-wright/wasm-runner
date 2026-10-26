pub fn handle(_body: &[u8]) -> anyhow::Result<Vec<u8>> {
    Ok(b"Hello, world!".to_vec())
}
