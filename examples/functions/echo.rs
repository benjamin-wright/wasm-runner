pub fn handle(body: &[u8]) -> anyhow::Result<Vec<u8>> {
    let value: serde_json::Value = serde_json::from_slice(body)?;
    Ok(serde_json::to_vec(&value)?)
}
