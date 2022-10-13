use std::io::{Cursor, Read};
use std::mem;
use std::string::FromUtf8Error;
use byteorder::{ReadBytesExt, LittleEndian};

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub  struct VBanPacket {
    four_c: [u8; 4],
    sub_protocol: u8,
    smp_per_frame: u8,
    channels: u8,
    data_format: u8,
    stream_name: [u8; 16],
    frame_cnt: u32
}


pub  const  HEADER_SIZE: usize = mem::size_of::<VBanPacket>();
const VBAN_MAGIC: [u8; 4] = *b"VBAN";

const VBAN_SRLIST: [i32; 21] =
[
    6000, 12000, 24000, 48000, 96000, 192000, 384000,
    8000, 16000, 32000, 64000, 128000, 256000, 512000,
    11025, 22050, 44100, 88200, 176400, 352800, 705600
];

#[derive(PartialEq)]
pub  enum VbanProtocol {
    AUDIO = 0x00,
    SERIAL = 0x20,
    TXT = 0x40,
    SERVICE = 0x60,
    UNDEF1 = 0x80,
    UNDEF2 = 0xA0,
    UNDEF3 = 0xC0,
    USER = 0xE0
}

#[derive(Debug, PartialEq)]
pub  enum VBanBitFormat {
    U8,
    I16,
    I24,
    I32,
    F32,
    F64,
    I12,
    I10
}

impl VBanPacket {
    // check if packet is valid
    pub  fn is_vban(&self) -> bool {
        self.four_c == VBAN_MAGIC
    }

    // get sample rate
    pub fn get_sr(&self) -> Result<i32, &'static str> {
        let smp_masked = (self.sub_protocol & 0b11111) as usize;
        return if smp_masked > VBAN_SRLIST.len() {
            Err("invalid subprotocol value")
        } else {
            Ok(VBAN_SRLIST[smp_masked])
        }
    }

    pub fn is_audio(&self) -> bool {
        self.get_protocol() == Some(VbanProtocol::AUDIO)
    }

    pub fn get_protocol(&self) -> Option<VbanProtocol> {
        let proto_masked = self.sub_protocol & 0b11100000;
        match proto_masked
        {
            0x00 => Some(VbanProtocol::AUDIO),
            0x20 => Some(VbanProtocol::SERIAL),
            0x40 => Some(VbanProtocol::TXT),
            0x60 => Some(VbanProtocol::SERVICE),
            0x80 => Some(VbanProtocol::UNDEF1),
            0xA0 => Some(VbanProtocol::UNDEF2),
            0xC0 => Some(VbanProtocol::UNDEF3),
            0xE0 => Some(VbanProtocol::USER),
            _ => None
        }
    }

    pub fn get_bit_format(&self) -> VBanBitFormat {
        let  bit_masked = self.data_format & 0b111;
        match bit_masked {
            0 => VBanBitFormat::U8,
            1 => VBanBitFormat::I16,
            2 => VBanBitFormat::I24,
            3 => VBanBitFormat::I32,
            4 => VBanBitFormat::F32,
            5 => VBanBitFormat::F64,
            6 => VBanBitFormat::I12,
            7 => VBanBitFormat::I10,
            _ => panic!("impossible")
        }
    }

    pub fn is_pcm(&self) -> bool {
        let bit_masked = self.data_format & 0b11111000;
        bit_masked == 0
    }

    pub fn stream_name(&self) -> Result<String, FromUtf8Error> {
        String::from_utf8(self.stream_name.to_vec())
    }

    pub fn from_bytes<'a>(bytes: &[u8; HEADER_SIZE]) -> VBanPacket {
        let mut crs = Cursor::new(bytes);

        // fourcc
        let mut four_c: [u8; 4] = [0; 4];
        crs.read_exact(&mut four_c).unwrap();

        let sub_protocol = crs.read_u8().unwrap();
        let smp_per_frame = crs.read_u8().unwrap();
        let channels = crs.read_u8().unwrap();
        let data_format = crs.read_u8().unwrap();
        let mut stream_name: [u8; 16] = [0; 16];
        crs.read_exact(&mut stream_name).unwrap();

        let frame_cnt = crs.read_u32::<LittleEndian>().unwrap();

        // this sucks
        VBanPacket {
            four_c,
            sub_protocol,
            smp_per_frame,
            channels,
            data_format,
            stream_name,
            frame_cnt
        }
    }
}