use std::fmt;
use binread::prelude::*;
use binread::{FilePtr16, FilePtr32, FilePtr64, NullString};

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

#[derive(BinRead, Debug)]
struct HeaderInner {
    revision: u16,

    #[br(parse_with = FilePtr32::parse, map = NullString::into_string)]
    file_name: String,

    #[br(pad_before = 2, parse_with = FilePtr16::parse)]
    str_addr: StrSection,

    #[br(parse_with = FilePtr32::parse)]
    reloc_addr: RelocationTable,

    file_size: u32,
}

#[derive(BinRead, Debug)]
struct RelocationSection {
    pointer: u64,
    position: u32,
    size: u32,
    index: u32,
    count: u32,
}

#[derive(BinRead, Debug)]
struct RelocationEntry {
    position: u32,
    struct_count: u16,
    offset_count: u8,
    padding_count: u8,
}

#[derive(BinRead, Debug)]
#[br(magic = b"_RLT")]
struct RelocationTable {
    rlt_section_pos: u32,
    count: u32,

    #[br(pad_before = 4, count = count)]
    sections: Vec<RelocationSection>,
    
    #[br(count = sections.iter().map(|x| x.count).sum::<u32>())]
    entries: Vec<RelocationEntry>,
}

#[derive(BinRead, Debug)]
#[br(magic = b"_STR")]
struct StrSection {
    unk: u32,
    unk2: u32,
    unk3: u32,
    str_count: u32,
    unk4: u32,

    #[br(count = str_count)]
    strings: Vec<BntxStr>,
}

#[derive(BinRead, Debug)]
struct BntxStr {
    len: u16,
    #[br(align_after = 4, count = len, map = |x: Vec<u8>| String::from_utf8_lossy(&x).into_owned())]
    chars: String,
}

impl Into<String> for BntxStr {
    fn into(self) -> String {
        self.chars
    }
}

#[derive(BinRead, Debug)]
#[br(magic = b"NX  ")]
struct NxHeader {
    count: u32,
    #[br(count = count, parse_with = FilePtr64::parse)]
    info_ptr: Vec<FilePtr64<BrtiSection>>,
    data_blk_ptr: u64,

    #[br(parse_with = FilePtr64::parse)]
    dict_ptr: DictSection,
    dict_size: u64,
}

#[derive(BinRead, Debug)]
#[br(magic = b"_DIC")]
struct DictSection {

}


#[derive(BinRead, Debug)]
enum SurfaceFormat {
    #[br(magic = 0x0b06u32)]
    R8G8B8A8_SRGB ,

    Unknown(u32),
}

#[derive(BinRead, Debug)]
#[br(magic = b"BRTI")]
struct BrtiSection {
    size: u32,
    size2: u64,
    flags: u8,
    dim: u8,
    tile_mode: u16,
    siwzzle: u16,
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

use binread::{io::{Read, Seek}, ReadOptions};


fn read_double_indirect<R: Read + Seek>(reader: &mut R, options: &ReadOptions, args: (u32,)) -> BinResult<ImageData> {

    let mut data = <FilePtr64<FilePtr64<ImageData>> as BinRead>::read_options(
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
        let info: &BrtiSection = &*self.nx_header.info_ptr[0];
        let data = &self.nx_header.info_ptr[0].texture.0[..];

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
}

#[cfg(test)]
mod tests {
    use binread::prelude::*;
    use binread::io::*;
    use super::BntxFile;

    #[test]
    fn try_parse() {
        let mut data = Cursor::new(&include_bytes!("/home/jam/Downloads/ester.bntx")[..]);

        let test: BntxFile = data.read_le().unwrap();

        dbg!(&test);

        test.to_image()
            .save("test.png");
    }
}
