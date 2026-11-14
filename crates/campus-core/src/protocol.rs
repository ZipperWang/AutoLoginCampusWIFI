use base64::{Engine, alphabet::Alphabet, engine::general_purpose};
use hmac::{Hmac, Mac};
use md5::Md5;
use serde::Serialize;
use sha1::{Digest, Sha1};
use zeroize::Zeroizing;

const ALPHABET: &str = "LVoJPiCN2R8G90yg+hmFHuacZ1OWMnrsSTXkYpUq/3dlbfKwv6xztjI7DeBE45QA";

#[derive(Serialize)]
struct Info<'a> {
    username: &'a str,
    password: &'a str,
    ip: &'a str,
    acid: &'a str,
    enc_ver: &'static str,
}

pub(super) fn login_fields(
    username: &str,
    password: &str,
    ip: &str,
    token: &str,
    acid: &str,
) -> (String, String, String) {
    let mut mac = Hmac::<Md5>::new_from_slice(token.as_bytes()).expect("HMAC支持任意长度密钥");
    mac.update(password.as_bytes());
    let digest = format!("{:x}", mac.finalize().into_bytes());
    let info = Info {
        username,
        password,
        ip,
        acid,
        enc_ver: "srun_bx1",
    };
    let json = Zeroizing::new(serde_json::to_string(&info).expect("字符串字段可序列化"));

    //按门户脚本的UTF16码元顺序组装小端字
    let mut words = Zeroizing::new(Vec::<u32>::new());
    let mut count = 0u32;
    for unit in json.encode_utf16() {
        let offset = count % 4;
        if offset == 0 {
            words.push(0);
        }
        *words.last_mut().unwrap() |= u32::from(unit) << (offset * 8);
        count += 1;
    }
    words.push(count);
    let mut key = [0u32; 4];
    for (i, unit) in token.encode_utf16().take(16).enumerate() {
        key[i / 4] |= u32::from(unit) << ((i % 4) * 8);
    }
    let len = words.len();
    let mut previous = words[len - 1];
    let mut sum = 0u32;
    for _ in 0..(6 + 52 / len) {
        sum = sum.wrapping_add(0x9e37_79b9);
        let selector = ((sum >> 2) & 3) as usize;
        for i in 0..len {
            let next = words[(i + 1) % len];
            let mix = ((previous >> 5) ^ (next << 2))
                .wrapping_add((next >> 3) ^ (previous << 4) ^ (sum ^ next))
                .wrapping_add(key[(i & 3) ^ selector] ^ previous);
            words[i] = words[i].wrapping_add(mix);
            previous = words[i];
        }
    }
    let bytes: Vec<_> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let engine = general_purpose::GeneralPurpose::new(
        &Alphabet::new(ALPHABET).expect("固定编码表有效"),
        general_purpose::PAD,
    );
    let info = format!("{{SRBX1}}{}", engine.encode(bytes));
    let mut checksum = Sha1::new();
    for field in [username, &digest, acid, ip, "200", "1", &info] {
        checksum.update(token.as_bytes());
        checksum.update(field.as_bytes());
    }
    (
        format!("{{MD5}}{digest}"),
        info,
        format!("{:x}", checksum.finalize()),
    )
}
