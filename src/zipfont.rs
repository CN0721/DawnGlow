//! 从字体压缩包（zip）里流式解出一个字体文件。
//!
//! 厂商字体包动辄几百 MB，整包读进内存会 OOM，所以这里按中央目录定位条目，
//! 只把选中的那个字体流式解压出来。仅支持 store(0) 与 deflate(8)。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// 文件是否为 zip（本地文件头签名）。
pub fn is_zip_file(path: &Path) -> bool {
    let mut sig = [0u8; 4];
    matches!(File::open(path).and_then(|mut f| f.read_exact(&mut sig)), Ok(()) if &sig == b"PK\x03\x04")
}

/// 从 zip 里挑一个最适合做界面的字体，解压到 `out`，返回条目名。
pub fn extract_font(zip_path: &Path, out: &Path) -> Result<String, String> {
    let mut f = File::open(zip_path).map_err(|e| e.to_string())?;
    let n = f.metadata().map_err(|e| e.to_string())?.len();
    if n < 22 {
        return Err("压缩包不完整".into());
    }

    // 尾部若干字节里找 EOCD（结尾记录），拿到中央目录位置。
    let tail_len = n.min(66_000);
    f.seek(SeekFrom::Start(n - tail_len))
        .map_err(|e| e.to_string())?;
    let mut tail = vec![0u8; tail_len as usize];
    f.read_exact(&mut tail).map_err(|e| e.to_string())?;
    let mut eocd = None;
    for i in (0..tail.len().saturating_sub(21)).rev() {
        if &tail[i..i + 4] == b"PK\x05\x06" {
            eocd = Some(i);
            break;
        }
    }
    let eocd = eocd.ok_or("不是有效的 zip 压缩包")?;
    let count = u16le(&tail, eocd + 10) as usize;
    let cd_off = u32le(&tail, eocd + 16) as u64;

    let cd_len = (n.saturating_sub(cd_off)).min(16_000_000) as usize;
    f.seek(SeekFrom::Start(cd_off)).map_err(|e| e.to_string())?;
    let mut cd = vec![0u8; cd_len];
    f.read_exact(&mut cd).map_err(|e| e.to_string())?;

    // (评分, 条目名, 本地头偏移, 压缩方式, 压缩后大小)
    let mut best: Option<(i64, String, u64, u16, u64)> = None;
    let mut p = 0usize;
    for _ in 0..count.min(200_000) {
        if p + 46 > cd.len() || &cd[p..p + 4] != b"PK\x01\x02" {
            break;
        }
        let method = u16le(&cd, p + 10);
        let csize = u32le(&cd, p + 20) as u64;
        let usize_ = u32le(&cd, p + 24) as u64;
        let name_len = u16le(&cd, p + 28) as usize;
        let extra_len = u16le(&cd, p + 30) as usize;
        let cmt_len = u16le(&cd, p + 32) as usize;
        let lho = u32le(&cd, p + 42) as u64;
        if p + 46 + name_len > cd.len() {
            break;
        }
        let name = String::from_utf8_lossy(&cd[p + 46..p + 46 + name_len]).to_string();
        p += 46 + name_len + extra_len + cmt_len;

        let low = name.to_lowercase();
        if !(low.ends_with(".ttf") || low.ends_with(".otf") || low.ends_with(".ttc")) {
            continue;
        }
        // 优先 Regular/Medium，避开细/粗/斜/等宽/非中文字集
        let mut score = usize_ as i64;
        if low.contains("regular") {
            score += 20_000_000;
        } else if low.contains("medium") || low.contains("normal") {
            score += 10_000_000;
        }
        if low.contains("bold") {
            score -= 4_000_000;
        }
        if low.contains("light")
            || low.contains("thin")
            || low.contains("black")
            || low.contains("heavy")
        {
            score -= 6_000_000;
        }
        if low.contains("italic") || low.contains("oblique") {
            score -= 8_000_000;
        }
        if low.contains("mono") || low.contains("latin") || low.contains("arabic") {
            score -= 12_000_000;
        }
        if best.as_ref().map_or(true, |b| score > b.0) {
            best = Some((score, name, lho, method, csize));
        }
    }
    let (_, name, lho, method, csize) = best.ok_or("压缩包里没有找到字体文件")?;

    f.seek(SeekFrom::Start(lho)).map_err(|e| e.to_string())?;
    let mut lh = [0u8; 30];
    f.read_exact(&mut lh).map_err(|e| e.to_string())?;
    if &lh[0..4] != b"PK\x03\x04" {
        return Err("压缩包条目已损坏".into());
    }
    let nl = u16le(&lh, 26) as u64;
    let el = u16le(&lh, 28) as u64;
    f.seek(SeekFrom::Start(lho + 30 + nl + el))
        .map_err(|e| e.to_string())?;

    let mut dst = File::create(out).map_err(|e| e.to_string())?;
    let mut src = f.take(csize);
    match method {
        0 => {
            std::io::copy(&mut src, &mut dst).map_err(|e| e.to_string())?;
        }
        8 => {
            let mut dec = flate2::read::DeflateDecoder::new(src);
            std::io::copy(&mut dec, &mut dst).map_err(|e| e.to_string())?;
        }
        m => return Err(format!("不支持的压缩方式（{m}）")),
    }
    dst.flush().map_err(|e| e.to_string())?;
    Ok(name)
}
