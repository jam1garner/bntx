use std::fmt;
use binread::prelude::*;
use binread::{FilePtr16, FilePtr32, FilePtr64, NullString};

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
    file_name: FilePtr32<NullString>,

    #[br(pad_before = 2)]
    str_addr: FilePtr16<StrSection>,
    reloc_addr: u32,
    file_size: u32,
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

#[derive(BinRead, Debug)]
#[br(magic = b"NX  ")]
struct NxHeader {
    count: u32,
    #[br(count = count)]
    info_ptr: FilePtr64<Vec<FilePtr64<BrtiSection>>>,
    data_blk_ptr: u64,
    dict_ptr: FilePtr64<DictSection>,
    dict_size: u64,
}

#[derive(BinRead, Debug)]
#[br(magic = b"_DIC")]
struct DictSection {

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
    format: u32,
    unk2: u32,
    width: u32,
    height: u32,
    depth: u32,
    array_len: u32,
    size_range: u32,
    unk4: [u32; 6],
    image_size: u32,
    align: u32,
    comp_sel: u32,
    ty: u32,
    name_addr: FilePtr64<BntxStr>,
    parent_addr: u64,

    #[br(args(image_size), parse_with = read_double_indirect)]
    textures: ImageData,
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

#[cfg(test)]
mod tests {
    use binread::prelude::*;
    use binread::io::*;
    use super::BntxFile;

    #[test]
    fn try_parse() {
        let mut data = Cursor::new(&include_bytes!("/home/jam/Downloads/ester.bntx")[..]);

        let test: BntxFile = data.read_le().unwrap();

        dbg!(test);
    }
}
