extern crate serial;
extern crate serial_core;

use std::io;
use std::io::{Read, Write, BufRead, BufReader};
use std::result::Result;
use std::convert::From;

enum BootloaderCommand {
    VerifyChecksum,
    GetFlashSize,
    GetAppStatus,
    EraseRow,
    Sync,
    SetActiveApp,
    SendData,
    EnterBootloader,
    ProgramRow,
    VerifyRow,
    ExitBootloader,
    GetMetaData,
}

impl Into<u8> for BootloaderCommand {
    fn into(self) -> u8 {
        match self {
            BootloaderCommand::VerifyChecksum => 0x31,
            BootloaderCommand::GetFlashSize => 0x32,
            BootloaderCommand::GetAppStatus => 0x33,
            BootloaderCommand::EraseRow => 0x34,
            BootloaderCommand::Sync => 0x35,
            BootloaderCommand::SetActiveApp => 0x36,
            BootloaderCommand::SendData => 0x37,
            BootloaderCommand::EnterBootloader => 0x38,
            BootloaderCommand::ProgramRow => 0x39,
            BootloaderCommand::VerifyRow => 0x3A,
            BootloaderCommand::ExitBootloader => 0x3B,
            BootloaderCommand::GetMetaData => 0x3C,
        }
    }
}

trait Bootloader: Read + Write + Sized {
    fn transmit(&mut self, tx_data: &[u8], response: bool) -> Result<Vec<u8>, Error> {
        self.write_all(tx_data)?;

        if response {
            let mut header = [0u8; 4];
            self.read_exact(&mut header)?;

            if header[0] != 0x01 {
                return Err(Error::Bootloader(BootloaderError::Data));
            }

            if header[1] != 0x00 {
                return Err(Error::Bootloader(BootloaderError::from(header[1])));
            }

            let len = (header[2] as u16) | ((header[3] as u16) << 8);
            let mut rx_data = Vec::new();
            Read::by_ref(self).take(len as u64).read_to_end(&mut rx_data)?;

            let mut footer = [0u8; 3];
            self.read_exact(&mut footer);

            let checksum: u16 = header.iter().chain(rx_data.iter()).fold(0u16, |a,b| a+(*b as u16));
            let checksum = 1 + !checksum;
            let packet_checksum = (footer[0] as u16) | ((footer[1] as u16) << 8);

            if packet_checksum != checksum {
                return Err(Error::Bootloader(BootloaderError::Checksum));
            }

            if footer[2] != 0x17 {
                return Err(Error::Bootloader(BootloaderError::Data));
            }

            Ok(rx_data)
        } else {
            Ok(Vec::new())
        }
    }

    fn create_packet(cmd: BootloaderCommand, data: Option<&[u8]>) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.push(0x01);
        packet.push(cmd.into());
        if let Some(data) = data {
            let len = data.len() as u16;
            packet.push(len as u8);
            packet.push((len >> 8) as u8);
            packet.extend_from_slice(data);
        } else {
            packet.push(0x00);
            packet.push(0x00);
        }
        let checksum: u16 = packet.iter().fold(0u16, |a,b| a+(*b as u16));
        let checksum = 1 + !checksum;
        packet.push(checksum as u8);
        packet.push((checksum >> 8) as u8);
        packet.push(0x17);
        packet
    }

    fn start_bootloader(&mut self, header: &CyacdHeader) -> Result<(), Error> {
        let packet = Self::create_packet(BootloaderCommand::EnterBootloader, None);
        let mut res = self.transmit(&packet, true)?;
        Ok(())
    }

    fn stop_bootloader(&mut self) -> Result<(), Error> {
        let packet = Self::create_packet(BootloaderCommand::ExitBootloader, None);
        let mut res = self.transmit(&packet, false)?;
        Ok(())
    }

    fn program_row(&mut self, row: &FlashRow) -> Result<(), Error> {
        let max_size = 50;
        let mut offset = 0;
        while row.data[offset..].len() > max_size {
            let start = offset as usize;
            let packet = Self::create_packet(BootloaderCommand::SendData, Some(&row.data[(offset as usize)..(offset as usize + max_size)]));
            self.transmit(&packet, true)?;
            offset += max_size;
        }

        let mut data = vec![row.array_id, row.row_num as u8, (row.row_num >> 8) as u8];
        data.extend_from_slice(&row.data[(offset as usize)..]);
        let packet = Self::create_packet(BootloaderCommand::ProgramRow, Some(&data));
        let mut res = self.transmit(packet.as_slice(), true)?;

        Ok(())
    }

    fn verify_row(&mut self, row: &FlashRow) -> Result<(), Error> {
        let mut data = vec![row.array_id, row.row_num as u8, (row.row_num >> 8) as u8];
        let packet = Self::create_packet(BootloaderCommand::VerifyRow, Some(&data));
        let mut res = self.transmit(packet.as_slice(), true)?;
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

impl From<u8> for BootloaderError {
    fn from(value: u8) -> BootloaderError {
        match value {
            0x03 => BootloaderError::Length,
            0x04 => BootloaderError::Data,
            0x05 => BootloaderError::Command,
            0x08 => BootloaderError::Checksum,
            0x09 => BootloaderError::Array,
            0x0A => BootloaderError::Row,
            0x0C => BootloaderError::App,
            0x0D => BootloaderError::Active,
            0x0E => BootloaderError::Callback,
            0x0F => BootloaderError::Unknown,
            _ => BootloaderError::Unknown,
        }
    }
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
    comm.open();
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
    use super::serial;
    use super::serial_core::{SerialDevice, SerialPortSettings, FlowControl};

    use std::fs::File;
    use std::io;
    use std::io::{Read, Write, BufRead, BufReader};
    use std::mem;
    use std::time::Duration;

    struct Comm {
        device: Option<serial::SystemPort>,
    }

    impl super::Connection for Comm {
        fn open(&mut self) -> bool {
            let mut device = serial::open("COM6").unwrap();
            device.set_timeout(Duration::from_secs(1)).unwrap();
            let mut settings = SerialDevice::read_settings(&device).unwrap();
            settings.set_flow_control(FlowControl::FlowNone);
            device.write_settings(&settings);
            mem::replace(&mut self.device, Some(device));
            true
        }

        fn close(&mut self) -> bool {
            mem::replace(&mut self.device, None);
            true
        }
    }

    impl Read for Comm {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if let Some(ref mut device) = self.device {
                device.read(buf)
            } else {
                Err(io::Error::new(io::ErrorKind::NotConnected, "serial connection lost"))
            }
        }
    }

    impl Write for Comm {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if let Some(ref mut device) = self.device {
                device.write(buf)
            } else {
                Err(io::Error::new(io::ErrorKind::NotConnected, "serial port closed"))
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            if let Some(ref mut device) = self.device {
                device.flush()
            } else {
                Err(io::Error::new(io::ErrorKind::NotConnected, "serial port closed"))
            }
        }
    }

    #[test]
    fn it_works() {
        super::bootload(File::open("Design01.cyacd").unwrap(), Comm {device: None}).unwrap();
    }
}
