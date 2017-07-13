use std::io;
use std::io::{Read, Write, BufRead, BufReader};
use std::result::Result;
use std::convert::From;

trait Bootloader: Read + Write {
    fn transmit(&mut self, data: &[u8]) -> Result<Vec<u8>, Error> {
        self.write(data)?;

        let mut recv = Vec::new();
        self.read_to_end(&mut recv)?;

        Ok(recv)
    }

    fn start_bootloader(&mut self, header: &CyacdHeader) -> Result<(), Error> {
        println!("STARTING");
        Ok(())
    }

    fn stop_bootloader(&mut self) -> Result<(), Error> {
        println!("STOPPING");
        Ok(())
    }

    fn program_row(&mut self, row: &FlashRow) -> Result<(), Error> {
        println!("PROGRAMMING ROW");
        Ok(())
    }

    fn verify_row(&mut self, row: &FlashRow) -> Result<(), Error> {
        println!("VERIFYING ROW");
        Ok(())
    }
}

impl<T> Bootloader for T
where
    T: Read + Write,
{
}

pub trait Connection: Read + Write {
    fn open(&mut self) -> bool;
    fn close(&mut self) -> bool;
}

#[derive(Debug)]
pub enum HostError {
    Eof,
    Length,
    Data,
    Command,
    Device(io::Error),
    Version,
    Checksum,
    Array,
    Row,
    Bootloader,
    Active,
    Unknown,
}

#[derive(Debug)]
pub enum BootloaderError {
    Length,
    Data,
    Command,
    Checksum,
    Array,
    Row,
    App,
    Active,
    Callback,
    Unknown,
}

#[derive(Debug)]
pub enum Error {
    Host(HostError),
    Bootloader(BootloaderError),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::Host(HostError::Device(error))
    }
}

enum ChecksumType {
    Sum,
    Crc,
}

struct CyacdHeader {
    silicon_id: u32,
    silicon_rev: u8,
    checksum_type: ChecksumType,
}

struct FlashRow {
    array_id: u8,
    row_num: u16,
    size: u16,
    data: Vec<u8>,
    checksum: u8,
}

fn from_ascii(input: &str) -> Vec<u8> {
    input
        .as_bytes()
        .chunks(2)
        .filter_map(|chunk| {
            u8::from_str_radix(String::from_utf8_lossy(chunk).as_ref(), 16).ok()
        })
        .collect()
}

fn parse_header<I>(input: &mut I) -> Result<CyacdHeader, Error>
where
    I: BufRead,
{
    let mut header = String::new();
    input.read_line(&mut header)?;

    let bytes = from_ascii(header.as_str());
    if bytes.len() != 6 {
        return Err(Error::Host(HostError::Length));
    }

    let silicon_id = (bytes[0] as u32) << 24 | (bytes[1] as u32) << 16 | (bytes[2] as u32) << 8 |
        (bytes[3] as u32);
    let silicon_rev = bytes[4];
    let checksum_type = match bytes[5] {
        0 => ChecksumType::Sum,
        1 => ChecksumType::Crc,
        _ => return Err(Error::Host(HostError::Checksum)),
    };

    Ok(CyacdHeader {
        silicon_id,
        silicon_rev,
        checksum_type,
    })
}


fn parse_row<I>(input: &mut I) -> Result<FlashRow, Error>
where
    I: BufRead,
{
    let mut row = String::new();
    input.read_line(&mut row)?;

    if row.len() == 0 {
        return Err(Error::Host(HostError::Eof));
    }

    let bytes = from_ascii(&row[1..]);

    if bytes.len() <= 6 {
        return Err(Error::Host(HostError::Length));
    }
    if &row[0..1] != ":" {
        return Err(Error::Host(HostError::Command));
    }

    let array_id = bytes[0];
    let row_num = (bytes[1] as u16) << 8 | bytes[2] as u16;
    let size = (bytes[3] as u16) << 8 | bytes[4] as u16;
    let checksum = bytes[bytes.len() - 1];

    if (size + 6) as usize != bytes.len() {
        return Err(Error::Host(HostError::Length));
    }

    let data = bytes.as_slice()[5..((size + 5) as usize)].to_vec();

    Ok(FlashRow {
        array_id,
        row_num,
        size,
        data,
        checksum,
    })
}

pub fn bootload<I, C>(input: I, mut comm: C) -> Result<(), Error>
where
    I: Read,
    C: Connection,
{
    let mut input = BufReader::new(input);

    let header = parse_header(&mut input)?;
    comm.start_bootloader(&header)?;

    loop {
        match parse_row(&mut input) {
            Ok(row) => {
                comm.program_row(&row)?;
                comm.verify_row(&row)?;
            }
            Err(Error::Host(HostError::Eof)) => {
                comm.stop_bootloader()?;
                comm.close();
                return Ok(());
            }
            Err(error) => {
                return Err(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io;
    use std::io::{Read, Write, BufRead, BufReader};

    struct Comm;

    impl super::Connection for Comm {
        fn open(&mut self) -> bool {
            true
        }

        fn close(&mut self) -> bool {
            true
        }
    }

    impl Read for Comm {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            Ok(0)
        }
    }

    impl Write for Comm {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(0)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn it_works() {
        super::bootload(File::open("test.cyacd").unwrap(), Comm {}).unwrap();
    }
}
