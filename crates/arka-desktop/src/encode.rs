//! Pengkodean `-EncodedCommand`: UTF-16LE → base64.
//!
//! Kenapa tidak mengirim perintah sebagai argumen biasa?
//!  * isi perintah sering memuat `"`, `'`, `$`, `&`, `%`, newline, dan
//!    karakter non-ASCII (emoji/日本語);
//!  * argumen baris perintah Windows di-*quote* ulang oleh `CreateProcess`
//!    dan PowerShell punya aturan *parsing* sendiri → quoting manual selalu
//!    rapuh (tanda kutip hilang, `%VAR%` mengembang, argumen terpecah).
//!
//! `-EncodedCommand` menerima base64 dari teks UTF-16LE, jadi tidak ada
//! *parsing* argumen sama sekali: satu argumen, utuh, encoding pasti.
//!
//! Modul ini murni (tanpa API Windows) supaya bisa diuji dari Linux/CI.

use base64::Engine as _;

/// Base64 standar (bukan URL-safe) dari [script] yang dikodekan UTF-16LE.
pub fn encode(script: &str) -> String {
    // `encode_utf16` menghasilkan unit u16; PowerShell menunggu LE.
    let mut bytes = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Kebalikan [`encode`] — dipakai unit test dan `--print-script` (dry run).
pub fn decode(encoded: &str) -> Result<String, String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|e| format!("base64 tidak valid: {e}"))?;
    if raw.len() % 2 != 0 {
        return Err("panjang UTF-16LE ganjil".into());
    }
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();
    String::from_utf16(&units).map_err(|e| format!("bukan UTF-16 valid: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ascii() {
        let s = "Get-ChildItem -Force";
        assert_eq!(decode(&encode(s)).unwrap(), s);
    }

    #[test]
    fn round_trip_quotes_and_newlines() {
        let s = "Write-Output \"a & b\"\r\nWrite-Output 'c %PATH%'\n";
        assert_eq!(decode(&encode(s)).unwrap(), s);
    }

    #[test]
    fn round_trip_emoji_and_japanese() {
        let s = "Write-Output '🚀 日本語 — é ñ'";
        assert_eq!(decode(&encode(s)).unwrap(), s);
    }

    #[test]
    fn emoji_becomes_surrogate_pair_not_utf8_garbage() {
        // 🚀 = U+1F680 → 2 unit u16 di UTF-16LE → 4 byte, bukan 4 byte UTF-8.
        let encoded = encode("🚀");
        let raw = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .unwrap();
        assert_eq!(raw.len(), 4);
        assert_eq!(u16::from_le_bytes([raw[0], raw[1]]), 0xD83D);
        assert_eq!(u16::from_le_bytes([raw[2], raw[3]]), 0xDE80);
    }

    #[test]
    fn output_is_plain_base64_ascii() {
        let encoded = encode("echo 日本語 and 🚀");
        assert!(encoded
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c)));
    }

    #[test]
    fn decode_rejects_broken_input() {
        assert!(decode("!!bukan base64!!").is_err());
        // 3 byte = panjang ganjil untuk UTF-16LE.
        let odd = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]);
        assert!(decode(&odd).is_err());
    }
}
