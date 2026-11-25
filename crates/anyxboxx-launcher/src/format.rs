//! Structural checks of the game's files, from their headers only: a truncated PE image or RPF7
//! archive fails here. These are checks of the container layout, not of the game's content.

/// How much of a file's start is read: enough for the PE header and section table.
pub const HEAD_BYTES: usize = 64 * 1024;

const DOS_MAGIC: &[u8; 2] = b"MZ";
const PE_SIGNATURE: &[u8; 4] = b"PE\0\0";
const SECTION_HEADER_BYTES: usize = 40;
const RPF7_MAGIC: &[u8; 4] = b"RPF7";
const RPF7_HEADER_BYTES: u64 = 16;
const RPF7_ENTRY_BYTES: u64 = 16;

/// Checks a PE image (`.exe`, `.dll`): DOS and NT headers, and every section's raw data inside
/// the file. `head` is the file's first bytes, `len` its full length.
pub fn check_pe(head: &[u8], len: u64) -> Result<(), String> {
    if head.get(..2) != Some(DOS_MAGIC) {
        return Err("not a PE image (no MZ header)".to_string());
    }
    let e_lfanew = read_u32(head, 0x3C).ok_or("DOS header cut short")? as usize;
    if head.get(e_lfanew..e_lfanew + 4) != Some(PE_SIGNATURE) {
        return Err("no PE signature where the DOS header points".to_string());
    }
    let coff = e_lfanew + 4;
    let section_count = read_u16(head, coff + 2).ok_or("COFF header cut short")? as usize;
    let optional_size = read_u16(head, coff + 16).ok_or("COFF header cut short")? as usize;
    let table = coff + 20 + optional_size;
    let table_end = table + section_count * SECTION_HEADER_BYTES;
    if table_end > head.len() {
        return Err(format!(
            "section table ends at byte {table_end}, beyond the {} bytes read",
            head.len()
        ));
    }
    for index in 0..section_count {
        let at = table + index * SECTION_HEADER_BYTES;
        let raw_size = read_u32(head, at + 16).ok_or("section header cut short")? as u64;
        let raw_at = read_u32(head, at + 20).ok_or("section header cut short")? as u64;
        if raw_size > 0 && raw_at + raw_size > len {
            return Err(format!(
                "section {index} ends at byte {} but the file has {len} bytes (truncated)",
                raw_at + raw_size
            ));
        }
    }
    Ok(())
}

/// Checks an RPF7 archive header: its magic, and that the entry and name tables it declares lie
/// inside the file (a file cut inside its tables fails).
pub fn check_rpf7(head: &[u8], len: u64) -> Result<(), String> {
    if head.get(..4) != Some(RPF7_MAGIC) {
        return Err("not an RPF7 archive (bad magic)".to_string());
    }
    let entries = read_u32(head, 4).ok_or("RPF7 header cut short")? as u64;
    let names = read_u32(head, 8).ok_or("RPF7 header cut short")? as u64;
    let tables_end = RPF7_HEADER_BYTES + entries * RPF7_ENTRY_BYTES + names;
    if tables_end > len {
        return Err(format!(
            "header tables end at byte {tables_end} but the file has {len} bytes (truncated)"
        ));
    }
    Ok(())
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    bytes
        .get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal PE image: one section of `raw_size` bytes at file offset `raw_at`.
    fn pe(raw_at: u32, raw_size: u32) -> Vec<u8> {
        let mut b = vec![0u8; 0x200];
        b[0..2].copy_from_slice(b"MZ");
        b[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(PE_SIGNATURE);
        // COFF header at 0x84: NumberOfSections at +2, SizeOfOptionalHeader at +16.
        b[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        b[0x94..0x96].copy_from_slice(&0u16.to_le_bytes());
        // The one section header starts after the 20-byte COFF header.
        let table = 0x84 + 20;
        b[table + 16..table + 20].copy_from_slice(&raw_size.to_le_bytes());
        b[table + 20..table + 24].copy_from_slice(&raw_at.to_le_bytes());
        b
    }

    #[test]
    fn complete_pe_passes() {
        let bytes = pe(0x200, 0x100);
        assert!(check_pe(&bytes, 0x300).is_ok());
    }

    #[test]
    fn truncated_pe_fails_on_its_section() {
        let bytes = pe(0x200, 0x100);
        let error = check_pe(&bytes, 0x250).expect_err("section runs past the end");
        assert!(error.contains("truncated"), "{error}");
    }

    #[test]
    fn non_pe_files_fail() {
        assert!(check_pe(b"GIF89a", 6).is_err());
        let mut bytes = pe(0x200, 0x100);
        bytes[0x80..0x84].copy_from_slice(b"XX\0\0");
        assert!(check_pe(&bytes, 0x300).is_err());
    }

    #[test]
    fn rpf7_with_tables_inside_passes() {
        let mut head = b"RPF7".to_vec();
        head.extend_from_slice(&2u32.to_le_bytes()); // entries
        head.extend_from_slice(&10u32.to_le_bytes()); // names
        head.extend_from_slice(&0u32.to_le_bytes());
        // 16 + 2 * 16 + 10 = 58 bytes of tables
        assert!(check_rpf7(&head, 58).is_ok());
    }

    #[test]
    fn rpf7_cut_inside_its_tables_fails() {
        let mut head = b"RPF7".to_vec();
        head.extend_from_slice(&2u32.to_le_bytes());
        head.extend_from_slice(&10u32.to_le_bytes());
        head.extend_from_slice(&0u32.to_le_bytes());
        let error = check_rpf7(&head, 40).expect_err("tables run past the end");
        assert!(error.contains("truncated"), "{error}");
    }

    #[test]
    fn rpf7_bad_magic_and_short_headers_fail() {
        assert!(check_rpf7(b"RPF2 and more bytes here", 30).is_err());
        assert!(check_rpf7(b"RPF7", 4).is_err());
    }
}
