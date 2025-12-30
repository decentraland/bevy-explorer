use std::io::Write;

pub async fn read_file(filename: &str) -> Result<Vec<u8>, anyhow::Error> {
    Ok(std::fs::read(filename)?)
}

pub async fn write_file(filename: &str, data: &[u8]) -> Result<(), anyhow::Error> {
    let mut file_output = std::fs::File::create(filename)?;

    Ok(file_output.write_all(data)?)
}
