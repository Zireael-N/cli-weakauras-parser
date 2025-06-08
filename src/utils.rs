use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Write},
};

pub(crate) fn read_from_file(path: &OsStr) -> Result<Vec<u8>, io::Error> {
    if path == "-" {
        let stdin = io::stdin();
        let mut stdin = BufReader::new(stdin.lock());

        let mut buf = Vec::with_capacity(1024);
        stdin.read_to_end(&mut buf)?;
        Ok(buf)
    } else {
        fs::read(path)
    }
}

pub(crate) fn write_to_file(path: Option<&OsStr>, data: &[u8]) -> Result<(), io::Error> {
    match path {
        None => {
            let stdout = io::stdout();
            let mut stdout = BufWriter::new(stdout.lock());

            stdout.write_all(data)?;
            stdout.flush()
        }
        Some(path) => {
            let mut file = BufWriter::new(File::create(path)?);

            file.write_all(data)?;
            file.flush()?;
            file.into_inner()?.sync_data()
        }
    }
}
