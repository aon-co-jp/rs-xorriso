//! 最小構成のISO9660(Level 1)イメージライター。
//!
//! GNU xorrisoへの敬意を込めたRust版(`rs-xorriso`)の最初の一歩として、
//! サブディレクトリを持たないフラットなファイル一覧からISO9660イメージを
//! 生成する機能のみを実装する。Rock Ridge/Joliet拡張は無く、ファイル名は
//! 8.3形式に切り詰められる(制限は README/CLAUDE.md に明記)。
//!
//! 参考仕様: ECMA-119 (ISO 9660)

use std::fs;
use std::io::{self, Write};
use std::path::Path;

const SECTOR_SIZE: usize = 2048;

/// ソースディレクトリ直下のファイル(サブディレクトリは対象外、警告のみ)から
/// ISO9660イメージを書き出す。
pub fn write_iso(source_dir: &Path, output_path: &Path, volume_label: &str) -> io::Result<()> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            eprintln!(
                "warning: サブディレクトリ '{}' はv0.1では未対応のためスキップします / \
                 subdirectories are not yet supported in v0.1, skipping",
                path.display()
            );
            continue;
        }
        let data = fs::read(&path)?;
        let name = sanitize_iso_name(&entry.file_name().to_string_lossy());
        files.push((name, data));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let image = build_image(&files, volume_label)?;
    let mut out = fs::File::create(output_path)?;
    out.write_all(&image)?;
    Ok(())
}

/// ファイル名をISO9660 Level 1(8.3形式・大文字英数字と`_`のみ)へ変換する。
fn sanitize_iso_name(name: &str) -> String {
    let upper: String = name
        .to_uppercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '.' { c } else { '_' })
        .collect();

    let (stem, ext) = match upper.rsplit_once('.') {
        Some((s, e)) => (s, e),
        None => (upper.as_str(), ""),
    };
    let stem: String = stem.chars().take(8).collect();
    let ext: String = ext.chars().take(3).collect();

    if ext.is_empty() {
        format!("{stem};1")
    } else {
        format!("{stem}.{ext};1")
    }
}

fn ceil_sectors(bytes: usize) -> usize {
    (bytes + SECTOR_SIZE - 1) / SECTOR_SIZE
}

fn both_endian_32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
    buf.extend_from_slice(&value.to_be_bytes());
}

fn both_endian_16(buf: &mut Vec<u8>, value: u16) {
    buf.extend_from_slice(&value.to_le_bytes());
    buf.extend_from_slice(&value.to_be_bytes());
}

fn pad_str(s: &str, len: usize) -> Vec<u8> {
    let mut bytes: Vec<u8> = s.bytes().take(len).collect();
    while bytes.len() < len {
        bytes.push(b' ');
    }
    bytes
}

/// ディレクトリレコード(root/親/子で共用)。
struct DirRecord {
    identifier: Vec<u8>, // 0x00=self, 0x01=parent, それ以外はファイル名
    is_directory: bool,
    extent_sector: u32,
    data_length: u32,
}

fn write_dir_record(buf: &mut Vec<u8>, rec: &DirRecord) {
    let id_len = rec.identifier.len() as u8;
    let pad = if id_len % 2 == 0 { 1 } else { 0 };
    let record_len = 33 + id_len as usize + pad;

    buf.push(record_len as u8); // length of this record
    buf.push(0); // extended attribute record length
    both_endian_32(buf, rec.extent_sector);
    both_endian_32(buf, rec.data_length);
    // recording date/time (7 bytes) — 2026年固定は避け、実行時刻は使わず0固定
    // (再現可能なビルドのため、xorrisoの--no-dateに相当する挙動)
    buf.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0]);
    buf.push(if rec.is_directory { 0x02 } else { 0x00 }); // file flags
    buf.push(0); // file unit size
    buf.push(0); // interleave gap size
    both_endian_16(buf, 1); // volume sequence number
    buf.push(id_len);
    buf.extend_from_slice(&rec.identifier);
    if pad == 1 {
        buf.push(0);
    }
}

fn build_image(files: &[(String, Vec<u8>)], volume_label: &str) -> io::Result<Vec<u8>> {
    // レイアウト: 16(system area) + PVD(1) + terminator(1) + pathtable L + pathtable M + root dir + files...
    const PVD_SECTOR: usize = 16;
    const TERM_SECTOR: usize = 17;
    const PATH_TABLE_L_SECTOR: usize = 18;

    // root用path tableは1エントリのみ(サブディレクトリ非対応のv0.1)。
    let path_table_bytes = {
        let mut b = Vec::new();
        b.push(1u8); // directory identifier length
        b.push(0); // extended attribute record length
        b.extend_from_slice(&0u32.to_le_bytes()); // extent (仮、後で埋める)
        b.extend_from_slice(&1u16.to_le_bytes()); // parent directory number
        b.push(0); // identifier (root = 0x00)
        b.push(0); // padding (len=1 is odd -> no padding needed, but keep even total: 1+1+4+2+1=9, add 1 pad)
        b
    };
    let path_table_sectors = ceil_sectors(path_table_bytes.len());
    let path_table_m_sector = PATH_TABLE_L_SECTOR + path_table_sectors;
    let root_dir_sector = path_table_m_sector + path_table_sectors;

    // ルートディレクトリのレコード(., .., ファイル群)を仮組みしてサイズを確定する。
    let mut file_layout: Vec<(String, u32, u32)> = Vec::new(); // (name, sector, size)
    let mut next_sector = root_dir_sector + 1; // rootディレクトリ自体は1セクタと仮定(小規模用途向け)
    for (name, data) in files {
        let sectors = ceil_sectors(data.len()).max(1);
        file_layout.push((name.clone(), next_sector as u32, data.len() as u32));
        next_sector += sectors;
    }
    let total_sectors = next_sector;

    let mut image = vec![0u8; total_sectors * SECTOR_SIZE];

    // Path table (L=little-endian, M=big-endian) — root extentを確定値で書き直す。
    let mut path_table_l = path_table_bytes.clone();
    path_table_l[2..6].copy_from_slice(&(root_dir_sector as u32).to_le_bytes());
    let mut path_table_m = path_table_bytes.clone();
    path_table_m[2..6].copy_from_slice(&(root_dir_sector as u32).to_be_bytes());
    // M-tableのparent directory numberはBE
    path_table_m[6..8].copy_from_slice(&1u16.to_be_bytes());

    image[PATH_TABLE_L_SECTOR * SECTOR_SIZE..PATH_TABLE_L_SECTOR * SECTOR_SIZE + path_table_l.len()]
        .copy_from_slice(&path_table_l);
    image[path_table_m_sector * SECTOR_SIZE..path_table_m_sector * SECTOR_SIZE + path_table_m.len()]
        .copy_from_slice(&path_table_m);

    // ルートディレクトリのレコード列を構築。
    let mut root_dir = Vec::new();
    write_dir_record(
        &mut root_dir,
        &DirRecord { identifier: vec![0x00], is_directory: true, extent_sector: root_dir_sector as u32, data_length: SECTOR_SIZE as u32 },
    );
    write_dir_record(
        &mut root_dir,
        &DirRecord { identifier: vec![0x01], is_directory: true, extent_sector: root_dir_sector as u32, data_length: SECTOR_SIZE as u32 },
    );
    for (name, sector, size) in &file_layout {
        write_dir_record(
            &mut root_dir,
            &DirRecord { identifier: name.clone().into_bytes(), is_directory: false, extent_sector: *sector, data_length: *size },
        );
    }
    if root_dir.len() > SECTOR_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "root directory exceeds one sector — too many files for rs-xorriso v0.1 (サブディレクトリ・複数セクタ未対応)",
        ));
    }
    image[root_dir_sector * SECTOR_SIZE..root_dir_sector * SECTOR_SIZE + root_dir.len()].copy_from_slice(&root_dir);

    // ファイル本体を配置。
    for ((_, sector, _), (_, data)) in file_layout.iter().zip(files.iter()) {
        let start = *sector as usize * SECTOR_SIZE;
        image[start..start + data.len()].copy_from_slice(data);
    }

    // Primary Volume Descriptor
    let mut pvd = Vec::with_capacity(SECTOR_SIZE);
    pvd.push(1); // volume descriptor type = primary
    pvd.extend_from_slice(b"CD001");
    pvd.push(1); // version
    pvd.push(0); // unused
    pvd.extend(pad_str("", 32)); // system identifier
    pvd.extend(pad_str(volume_label, 32)); // volume identifier
    pvd.extend_from_slice(&[0u8; 8]); // unused
    both_endian_32(&mut pvd, total_sectors as u32); // volume space size
    pvd.extend_from_slice(&[0u8; 32]); // unused
    both_endian_16(&mut pvd, 1); // volume set size
    both_endian_16(&mut pvd, 1); // volume sequence number
    both_endian_16(&mut pvd, SECTOR_SIZE as u16); // logical block size
    both_endian_32(&mut pvd, path_table_bytes.len() as u32); // path table size
    pvd.extend_from_slice(&(PATH_TABLE_L_SECTOR as u32).to_le_bytes()); // type L path table location
    pvd.extend_from_slice(&0u32.to_le_bytes()); // optional type L
    pvd.extend_from_slice(&(path_table_m_sector as u32).to_be_bytes()); // type M path table location
    pvd.extend_from_slice(&0u32.to_be_bytes()); // optional type M

    write_dir_record(
        &mut pvd,
        &DirRecord { identifier: vec![0x00], is_directory: true, extent_sector: root_dir_sector as u32, data_length: SECTOR_SIZE as u32 },
    );

    pvd.extend(pad_str("", 128)); // volume set identifier
    pvd.extend(pad_str("RS-XORRISO", 128)); // publisher identifier
    pvd.extend(pad_str("", 128)); // data preparer identifier
    pvd.extend(pad_str("RS-XORRISO", 128)); // application identifier
    pvd.extend(pad_str("", 37)); // copyright file identifier
    pvd.extend(pad_str("", 37)); // abstract file identifier
    pvd.extend(pad_str("", 37)); // bibliographic file identifier
    pvd.extend_from_slice(&unspecified_datetime17()); // creation
    pvd.extend_from_slice(&unspecified_datetime17()); // modification
    pvd.extend_from_slice(&unspecified_datetime17()); // expiration
    pvd.extend_from_slice(&unspecified_datetime17()); // effective
    pvd.push(1); // file structure version
    pvd.push(0); // reserved

    while pvd.len() < SECTOR_SIZE {
        pvd.push(0);
    }
    assert_eq!(pvd.len(), SECTOR_SIZE, "PVD must be exactly one sector");
    image[PVD_SECTOR * SECTOR_SIZE..PVD_SECTOR * SECTOR_SIZE + SECTOR_SIZE].copy_from_slice(&pvd);

    // Volume Descriptor Set Terminator
    let mut term = vec![0u8; SECTOR_SIZE];
    term[0] = 255;
    term[1..6].copy_from_slice(b"CD001");
    term[6] = 1;
    image[TERM_SECTOR * SECTOR_SIZE..TERM_SECTOR * SECTOR_SIZE + SECTOR_SIZE].copy_from_slice(&term);

    Ok(image)
}

fn unspecified_datetime17() -> [u8; 17] {
    let mut d = [b'0'; 17];
    d[16] = 0; // GMT offset
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_long_names_to_8_3() {
        assert_eq!(sanitize_iso_name("my-song.mp3"), "MY_SONG.MP3;1");
        assert_eq!(sanitize_iso_name("verylongfilename.txt"), "VERYLONG.TXT;1");
        assert_eq!(sanitize_iso_name("README"), "README;1");
    }

    #[test]
    fn image_has_valid_cd001_magic_at_pvd_and_terminator() {
        let files = vec![("HELLO.TXT;1".to_string(), b"hello world".to_vec())];
        let image = build_image(&files, "TESTVOL").unwrap();
        assert_eq!(&image[16 * SECTOR_SIZE + 1..16 * SECTOR_SIZE + 6], b"CD001");
        assert_eq!(&image[17 * SECTOR_SIZE + 1..17 * SECTOR_SIZE + 6], b"CD001");
        assert_eq!(image[17 * SECTOR_SIZE], 255);
    }

    #[test]
    fn image_length_is_whole_number_of_sectors() {
        let files = vec![("A.TXT;1".to_string(), vec![1u8; 5000])];
        let image = build_image(&files, "TESTVOL").unwrap();
        assert_eq!(image.len() % SECTOR_SIZE, 0);
    }

    #[test]
    fn volume_identifier_is_embedded_in_pvd() {
        let files: Vec<(String, Vec<u8>)> = vec![];
        let image = build_image(&files, "MAKE_DISK").unwrap();
        let pvd = &image[16 * SECTOR_SIZE..17 * SECTOR_SIZE];
        let vol_id = &pvd[40..72];
        assert!(String::from_utf8_lossy(vol_id).starts_with("MAKE_DISK"));
    }
}
