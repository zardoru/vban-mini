use std::io::{Cursor, Read, Write};
use std::mem;
use byteorder::{ReadBytesExt, LittleEndian, WriteBytesExt};

#[derive(Debug, Copy, Clone)]
pub struct VBanPacket {
    four_c: [u8; 4],
    sub_protocol: u8,
    smp_count: u8,
    channels: u8,
    data_format: u8,
    stream_name: [u8; 16],
    frame_ordering: u32
}




pub const  HEADER_SIZE: usize = mem::size_of::<VBanPacket>();
const VBAN_MAGIC: [u8; 4] = *b"VBAN";
// max size of udp contents given an MTU of 1500
pub const VBAN_MAX_PACKET_SIZE: usize = 1472usize;

const VBAN_SRLIST: [i32; 21] =
[
    6000, 12000, 24000, 48000, 96000, 192000, 384000,
    8000, 16000, 32000, 64000, 128000, 256000, 512000,
    11025, 22050, 44100, 88200, 176400, 352800, 705600
];

#[derive(PartialEq)]
pub enum VbanProtocol {
    AUDIO = 0x00,
    SERIAL = 0x20,
    TXT = 0x40,
    SERVICE = 0x60,
    UNDEF1 = 0x80,
    UNDEF2 = 0xA0,
    UNDEF3 = 0xC0,
    USER = 0xE0
}

impl From<u8> for VbanProtocol {
    fn from(val: u8) -> Self {
        match val {
            0x00 => VbanProtocol::AUDIO,
            0x20 => VbanProtocol::SERIAL,
            0x40 => VbanProtocol::TXT,
            0x60 => VbanProtocol::SERVICE,
            0x80 => VbanProtocol::UNDEF1,
            0xA0 => VbanProtocol::UNDEF2,
            0xC0 => VbanProtocol::UNDEF3,
            0xE0 => VbanProtocol::USER,
            _ => panic!("bad protocol")
        }
    }
}

impl From<VbanProtocol> for u8 {
    fn from(val: VbanProtocol) -> Self {
        match val {
            VbanProtocol::AUDIO => 0x00,
            VbanProtocol::SERIAL => 0x20,
            VbanProtocol::TXT => 0x40,
            VbanProtocol::SERVICE => 0x60,
            VbanProtocol::UNDEF1 => 0x80,
            VbanProtocol::UNDEF2 => 0xA0,
            VbanProtocol::UNDEF3 => 0xC0,
            VbanProtocol::USER => 0xE0
        }
    }
}


#[derive(Debug, PartialEq)]
pub enum VBanBitFormat {
    U8,
    I16,
    I24,
    I32,
    F32,
    F64,
    I12,
    I10
}

impl From<u8> for VBanBitFormat {
    fn from(bits: u8) -> Self {
        match bits {
            0 => VBanBitFormat::U8,
            1 => VBanBitFormat::I16,
            2 => VBanBitFormat::I24,
            3 => VBanBitFormat::I32,
            4 => VBanBitFormat::F32,
            5 => VBanBitFormat::F64,
            6 => VBanBitFormat::I12,
            7 => VBanBitFormat::I10,
            _ => panic!("bad bit format")
        }
    }
}

impl From<VBanBitFormat> for u8 {
    fn from(it: VBanBitFormat) -> u8 {
        match it {
            VBanBitFormat::U8 => 0,
            VBanBitFormat::I16 => 1,
            VBanBitFormat::I24 => 2,
            VBanBitFormat::I32 => 3,
            VBanBitFormat::F32 => 4,
            VBanBitFormat::F64 => 5,
            VBanBitFormat::I12 => 6,
            VBanBitFormat::I10 => 7,
        }
    }
}



impl VBanPacket {
    // check if packet is valid
    pub fn is_vban(&self) -> bool {
        self.four_c == VBAN_MAGIC
    }

    pub fn make_audio_packet(
        sample_rate: i32,
        stream_name: [u8; 16],
        smp_count: u16,
        channels: u8,
        stream_data: &[u8],
        bit_fmt: VBanBitFormat,
        frame_ordering: u32
    ) -> Vec<u8> {
        let zero = 0;
        let sr_index =
            VBAN_SRLIST.iter()
                .enumerate()
                .find(|&it| *it.1 == sample_rate)
                .unwrap_or((usize::MAX, &zero)).0;

        if sr_index == usize::MAX {
            panic!("tried to make a packet with unsupport sample rate ({})", sample_rate)
        }

        let packet = VBanPacket {
            four_c: VBAN_MAGIC.clone(),
            /* we would bitwise-or this with a packet type, but it's 0 for audio.
               so we just use the sr index.
            */
            sub_protocol: sr_index as u8,
            smp_count: (smp_count - 1) as u8,
            channels: channels - 1,

            /* bits 4/7 unused: always pcm */
            data_format: bit_fmt.into(),
            stream_name,
            frame_ordering,
        };
        let mut full_packet = Cursor::new(Vec::<u8>::new());

        full_packet.write(packet.to_bytes().as_slice()).expect("out of memory");
        full_packet.write(stream_data).expect("out of memory");

        full_packet.into_inner()
    }

    /// convert header into bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut c = Cursor::new(Vec::<u8>::new());

        c.write(&self.four_c).unwrap();
        c.write_u8(self.sub_protocol).unwrap();
        c.write_u8(self.smp_count).unwrap();
        c.write_u8(self.channels).unwrap();
        c.write_u8(self.data_format).unwrap();
        c.write(&self.stream_name).unwrap();
        c.write_u32::<LittleEndian>(self.frame_ordering).unwrap();

        c.into_inner()
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
        let v = VbanProtocol::from(proto_masked);

        Some(v)
    }

    pub fn get_bit_format(&self) -> VBanBitFormat {
        let  bit_masked = self.data_format & 0b111;
        bit_masked.into()
    }

    pub fn is_pcm(&self) -> bool {
        let bit_masked = self.data_format & 0b11111000;
        bit_masked == 0
    }

    pub fn get_channels(&self) -> u8 {
        self.channels + 1
    }

    pub fn get_seq_num(&self) -> u32 {
        self.frame_ordering
    }
    pub fn get_stream_name(&self) -> String {
        let nul_idx = self.stream_name.iter()
            .position(|x| *x == 0u8)
            .unwrap_or(self.stream_name.len());

        String::from_utf8_lossy(&self.stream_name[0..nul_idx]).into()
    }
    pub fn get_frame_count(&self) -> u8 {
        self.smp_count + 1
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
            smp_count: smp_per_frame,
            channels,
            data_format,
            stream_name,
            frame_ordering: frame_cnt
        }
    }
}