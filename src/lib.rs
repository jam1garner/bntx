use std::{fmt, io};
use std::path::Path;
use binread::prelude::*;
use binread::derive_binread;
use binread::{FilePtr16, FilePtr32, FilePtr64, NullString};

use binwrite::{BinWrite, WriterOption};

pub mod tegra_swizzle;

#[derive(BinRead, PartialEq, Debug, Clone, Copy)]
enum ByteOrder {
    #[br(magic = 0xFFFEu16)]
    LittleEndian,
    #[br(magic = 0xFEFFu16)]
    BigEndian,
}

#[derive(BinRead, Debug)]
#[br(magic = b"BNTX")]
struct BntxHeader {
    #[br(pad_before = 4)]
    version: (u16, u16),

    #[br(big)]
    bom: ByteOrder,

    #[br(is_little = bom == ByteOrder::LittleEndian)]
    inner: HeaderInner,
}

const BNTX_HEADER_SIZE: usize = 0x20;
const NX_HEADER_SIZE: usize = 0x28;
const HEADER_SIZE: usize = BNTX_HEADER_SIZE + NX_HEADER_SIZE;
const MEM_POOL_SIZE: usize = 0x150;
const DATA_PTR_SIZE: usize = 8;

const START_OF_STR_SECTION: usize = HEADER_SIZE + MEM_POOL_SIZE + DATA_PTR_SIZE;

const STR_HEADER_SIZE: usize = 0x14;
const EMPTY_STR_SIZE: usize = 4;

const FILENAME_STR_OFFSET: usize = START_OF_STR_SECTION + STR_HEADER_SIZE + EMPTY_STR_SIZE;

const BRTD_SECTION_START: usize = 0xFF0;
const START_OF_TEXTURE_DATA: usize = BRTD_SECTION_START + 0x10;

impl BntxHeader {
    fn write_options<W: io::Write>(
        &self,
        writer: &mut W,
        options: &WriterOption,
        parent: &BntxFile
    ) -> io::Result<()> {
        let start_of_reloc_section = (
            START_OF_TEXTURE_DATA + parent.nx_header.info_ptr.texture.0.len()
        ) as u32;
        (
            b"BNTX",
            0u32,
            self.version,
            match self.bom {
                ByteOrder::LittleEndian => b"\xFF\xFE",
                ByteOrder::BigEndian => b"\xFE\xFF",
            },
            self.inner.revision,
            FILENAME_STR_OFFSET as u32 + 2,
            0u16,
            START_OF_STR_SECTION as u16,
            start_of_reloc_section,
            start_of_reloc_section + (self.inner.reloc_table.get_size() as u32),
        ).write_options(writer, options)
    }
}

#[derive_binread]
#[derive(Debug)]
struct HeaderInner {
    revision: u16,

    #[br(parse_with = FilePtr32::parse, map = NullString::into_string)]
    file_name: String,

    #[br(pad_before = 2, parse_with = FilePtr16::parse)]
    str_section: StrSection,

    #[br(parse_with = FilePtr32::parse)]
    reloc_table: RelocationTable,

    #[br(temp)]
    file_size: u32,
}

#[derive(BinRead, BinWrite, Debug)]
struct RelocationSection {
    pointer: u64,
    position: u32,
    size: u32,
    index: u32,
    count: u32,
}

const SIZE_OF_RELOC_SECTION: usize = size_of::<u64>() + (size_of::<u32>() * 4);

#[derive(BinRead, BinWrite, Debug)]
struct RelocationEntry {
    position: u32,
    struct_count: u16,
    offset_count: u8,
    padding_count: u8,
}

const SIZE_OF_RELOC_ENTRY: usize = size_of::<u32>() + size_of::<u16>() + (size_of::<u8>() * 2);

#[derive_binread]
#[derive(Debug)]
#[br(magic = b"_RLT")]
struct RelocationTable {
    #[br(temp)]
    rlt_section_pos: u32,
    
    #[br(temp)]
    count: u32,

    #[br(pad_before = 4, count = count)]
    sections: Vec<RelocationSection>,
    
    #[br(count = sections.iter().map(|x| x.count).sum::<u32>())]
    entries: Vec<RelocationEntry>,
}

use core::mem::size_of;

impl RelocationTable {
    fn get_size(&self) -> usize {
        b"_RLT".len() +
        size_of::<u32>() +
        size_of::<u32>() +
        size_of::<u32>() +
        (self.sections.len() * SIZE_OF_RELOC_SECTION) +
        (self.entries.len() * SIZE_OF_RELOC_ENTRY)
    }

    fn write_options<W: io::Write>(&self, writer: &mut W, options: &WriterOption, parent: &BntxFile) -> io::Result<()> {
        (
            b"_RLT",
            (START_OF_TEXTURE_DATA + parent.nx_header.info_ptr.texture.0.len()) as u32,
            self.sections.len() as u32,
            0u32,
            &self.sections,
            &self.entries
        ).write_options(writer, options)
    }
}

#[derive_binread]
#[derive(Debug)]
#[br(magic = b"_STR")]
struct StrSection {
    unk: u32,
    unk2: u32,
    unk3: u32,

    #[br(temp)]
    str_count: u32,

    #[br(temp)]
    empty: BntxStr,

    #[br(count = str_count)]
    strings: Vec<BntxStr>,
}

impl BinWrite for StrSection {
    fn write_options<W: io::Write>(&self, writer: &mut W, options: &WriterOption) -> io::Result<()> {
        (
            b"_STR",
            self.unk,
            self.unk2,
            self.unk3,
            self.strings.len() as u32,
            BntxStr::from(String::new()),
            &self.strings,
        ).write_options(writer, options)
    }
}

impl StrSection {
    fn get_size(&self) -> usize {
        (5 * size_of::<u32>())
            + EMPTY_STR_SIZE
            + self.strings.iter()
                .map(|x| x.get_size())
                .sum::<usize>()
    }
}

#[derive_binread]
#[derive(BinWrite, Debug)]
struct BntxStr {
    len: u16,

    #[br(align_after = 4, count = len, map = |x: Vec<u8>| String::from_utf8_lossy(&x).into_owned())]
    #[binwrite(cstr, align_after(4))]
    chars: String,
}

fn align(x: usize, n: usize) -> usize {
    (x + n - 1) & !(n - 1)
}

impl BntxStr {
    fn get_size(&self) -> usize {
        align(
            size_of::<u16>()
                + self.chars.bytes().len()
                + 1,
            4
        )
    }
}

impl From<String> for BntxStr {
    fn from(chars: String) -> Self {
        BntxStr {
            len: chars.len() as u16,
            chars
        }
    }
}

impl From<BntxStr> for String {
    fn from(bntx_str: BntxStr) -> String {
        bntx_str.chars
    }
}

#[derive_binread]
#[derive(Debug)]
#[br(magic = b"NX  ")]
struct NxHeader {
    #[br(temp)]
    count: u32,

    #[br(parse_with = read_double_indirect)]
    info_ptr: BrtiSection,

    #[br(temp)]
    data_blk_ptr: u64,

    #[br(parse_with = FilePtr64::parse)]
    dict: DictSection,
    dict_size: u64,
}

impl NxHeader {
    fn write_options<W: io::Write>(
        &self,
        writer: &mut W,
        options: &WriterOption,
        parent: &BntxFile
    ) -> io::Result<()> {
        (
            b"NX  ",
            1u32, // count
            (HEADER_SIZE + MEM_POOL_SIZE) as u64,
            BRTD_SECTION_START as u64,
            (START_OF_STR_SECTION + parent.header.inner.str_section.get_size()) as u64,
            self.dict_size,
        ).write_options(writer, options)
    }
}

#[derive(BinRead, Debug)]
#[br(magic = b"_DIC")]
struct DictSection {
    // lol
}

static DICT_SECTION: &[u8] = b"\x5F\x44\x49\x43\x01\x00\x00\x00\xFF\xFF\xFF\xFF\x01\x00\x00\x00\xB4\x01\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\x00\x01\x00\xB8\x01\x00\x00\x00\x00\x00\x00";

impl DictSection {
    fn get_size(&self) -> usize {
        DICT_SECTION.len()
    }
}

impl BinWrite for DictSection {
    fn write_options<W: io::Write>(&self, writer: &mut W, options: &WriterOption) -> io::Result<()> {
        DICT_SECTION.write_options(writer, options)
    }
}

#[derive(BinRead, Debug)]
enum SurfaceFormat {
    #[br(magic = 0x0b06u32)]
    R8G8B8A8_SRGB,

    Unknown(u32),
}

impl BinWrite for SurfaceFormat {
    fn write_options<W: io::Write>(&self, writer: &mut W, options: &WriterOption) -> io::Result<()> {
        match self {
            SurfaceFormat::R8G8B8A8_SRGB => 0x0b06,
            SurfaceFormat::Unknown(x) => *x,
        }.write_options(writer, options)
    }
}

#[derive(BinRead, Debug)]
#[br(magic = b"BRTI")]
struct BrtiSection {
    size: u32,
    size2: u64,
    flags: u8,
    dim: u8,
    tile_mode: u16,
    swizzle: u16,
    mips_count: u16,
    num_multi_sample: u32,
    format: SurfaceFormat,
    unk2: u32,
    width: u32,
    height: u32,
    depth: u32,
    array_len: u32,
    size_range: i32,
    unk4: [u32; 6],
    image_size: u32,
    align: u32,
    comp_sel: u32,
    ty: u32,

    #[br(parse_with = FilePtr64::parse)]
    name_addr: BntxStr,
    parent_addr: u64,

    #[br(args(image_size), parse_with = read_double_indirect)]
    texture: ImageData,
}

const SIZE_OF_BRTI: usize = 0xA0;

impl BrtiSection {
    fn write_options<W: io::Write>(&self, writer: &mut W, options: &WriterOption, parent: &BntxFile) -> io::Result<()> {
        (
            (
                b"BRTI",
                self.size,
                self.size2,
                self.flags,
                self.dim,
                self.tile_mode,
                self.swizzle,
                self.mips_count,
                self.num_multi_sample,
                &self.format,
                self.unk2,
                self.width,
                self.height,
                self.depth,
                self.array_len,
                self.size_range,
                self.unk4,
                self.image_size,
                self.align,
                self.comp_sel,
            ),
            self.ty,
            FILENAME_STR_OFFSET as u64,
            BNTX_HEADER_SIZE as u64,
            (
                START_OF_STR_SECTION +
                parent.header.inner.str_section.get_size() +
                parent.nx_header.dict.get_size() +
                SIZE_OF_BRTI +
                0x200
            ) as u64,
            0u64,
            (
                START_OF_STR_SECTION +
                parent.header.inner.str_section.get_size() +
                parent.nx_header.dict.get_size() +
                SIZE_OF_BRTI
            ) as u64,
            (
                START_OF_STR_SECTION +
                parent.header.inner.str_section.get_size() +
                parent.nx_header.dict.get_size() +
                SIZE_OF_BRTI +
                0x100
            ) as u64,
            0u64,
            0u64
        ).write_options(writer, options)
    }
}

use binread::{io::{Read, Seek}, ReadOptions};

fn read_double_indirect<T: BinRead, R: Read + Seek>(
    reader: &mut R,
    options: &ReadOptions,
    args: T::Args
) -> BinResult<T> {

    let mut data = <FilePtr64<FilePtr64<T>> as BinRead>::read_options(
        reader,
        options,
        args
    )?;

    data.after_parse(reader, options, args)?;

    Ok(data.into_inner().into_inner())
}

#[derive(BinRead)]
#[br(import(len: u32))]
struct ImageData(#[br(count = len, parse_with = binread::helpers::read_bytes)] pub Vec<u8>);

impl fmt::Debug for ImageData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ImageData[{}]", self.0.len())
    }
}

#[derive(BinRead, Debug)]
struct BntxFile {
    header: BntxHeader,

    #[br(is_little = header.bom == ByteOrder::LittleEndian)]
    nx_header: NxHeader,
}

impl BntxFile {
    fn to_image(&self) -> image::DynamicImage {
        let info: &BrtiSection = &self.nx_header.info_ptr;
        let data = &self.nx_header.info_ptr.texture.0[..];

        let data = tegra_swizzle::deswizzle(
            info.width, info.height, info.depth,
            1,
            1,
            1,
            false,
            4,
            info.tile_mode as _,
            info.size_range,
            &info.texture.0
        );

        let base_size = info.width as usize * info.height as usize * 4;
        image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(info.width, info.height, data[..base_size].to_owned())
                .unwrap()
        )
    }

    fn write<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let options = binwrite::writer_option_new!(endian: binwrite::Endian::Little);
        self.header.write_options(writer, &options, self)?;
        self.nx_header.write_options(writer, &options, self)?;

        (
            // memory pool
            &[0u8; 0x150][..],
            (
                START_OF_STR_SECTION
                    + self.header.inner.str_section.get_size()
                    + self.nx_header.dict.get_size()
            ) as u64,
            &self.header.inner.str_section,
            &self.nx_header.dict,
        ).write_options(writer, &options)?;


        self.nx_header.info_ptr.write_options(writer, &options, self)?;

        (
            &[0; 0x100][..],
            &[0; 0x100][..],
        ).write_options(writer, &options)?;

        0x1000u64.write_options(writer, &options)?;

        let padding_size = BRTD_SECTION_START - (
            START_OF_STR_SECTION +
            self.header.inner.str_section.get_size() +
            self.nx_header.dict.get_size() +
            SIZE_OF_BRTI +
            0x200 +
            DATA_PTR_SIZE
        );

        vec![0u8; padding_size].write_options(writer, &options)?;

        // BRTD
        (
            b"BRTD",
            0,
            self.nx_header.info_ptr.texture.0.len() as u64 + 0x10
        ).write_options(writer, &options)?;
        

        writer.write_all(&self.nx_header.info_ptr.texture.0)?;

        self.header.inner.reloc_table.write_options(writer, &options, self)?;

        Ok(())
    }

    fn from_image(img: image::DynamicImage, name: &str) -> Self {
        let img = img.to_rgba();

        let (height, width) = img.dimensions();
        
        let data = tegra_swizzle::swizzle(
            width, height, 1,
            1,
            1,
            1,
            false,
            4,
            0,
            4,
            &img.into_raw()
        );

        BntxFile {
            header: BntxHeader {
                version: (0, 4),
                bom: ByteOrder::LittleEndian,
                inner: HeaderInner {
                    revision: 0x400c,
                    file_name: name.into(),
                    str_section: StrSection {
                        unk: 0x48,
                        unk2: 0x48,
                        unk3: 0,
                        strings: vec![BntxStr::from(name.to_owned())],
                    },
                    reloc_table: RelocationTable {
                        sections: vec![],
                        entries: vec![]
                    }
                }
            },
            nx_header: NxHeader {
                dict: DictSection {},
                dict_size: 0x58,
                info_ptr: BrtiSection {
                    size: 3592,
                    size2: 3592,
                    flags: 1,
                    dim: 2,
                    tile_mode: 0,
                    swizzle: 0,
                    mips_count: 0,
                    num_multi_sample: 1,
                    format: SurfaceFormat::R8G8B8A8_SRGB,
                    unk2: 32,
                    width,
                    height,
                    depth: 1,
                    array_len: 1,
                    size_range: 4,
                    unk4: [
                        65543,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ],
                    image_size: 4 * height * width,
                    align: 512,
                    comp_sel: 84148994,
                    ty: 1,
                    name_addr: name.to_owned().into(),
                    parent_addr: 32,
                    texture: ImageData(data)
                }
            }
        }
    }

    fn save<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let mut file = std::fs::File::create(path.as_ref())?;

        self.write(&mut file)
    }
}

#[cfg(test)]
mod tests {
    use binread::prelude::*;
    use binread::io::*;
    use super::BntxFile;

    #[test]
    fn try_parse() {
        //let mut data = Cursor::new(&include_bytes!("/home/jam/Downloads/ester.bntx")[..]);
        let mut data = Cursor::new(&include_bytes!("/home/jam/dev/ult/bntx/test.bntx")[..]);

        let test: BntxFile = data.read_le().unwrap();

        dbg!(&test);

        test.to_image()
            .save("test.png");
    }

    #[test]
    fn try_from_png() {
        let image = image::open("/home/jam/Pictures/smash_custom_skins.png").unwrap();

        let tex = BntxFile::from_image(image, "test");

        tex.save("test.bntx").unwrap();
    }
}
