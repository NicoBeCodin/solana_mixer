use serde::Deserialize;
use std::fs;
use std::error::Error;
use groth16_solana::groth16::Groth16Verifyingkey;
use num_bigint::BigUint;
use num_traits::Num; // Allows `.from_str_radix()`
use std::str::FromStr;

#[derive(Deserialize, Debug)]
struct RawVk {
    protocol: String,
    curve: String,
    nPublic: usize,
    vk_alpha_1: Vec<String>,
    vk_beta_2: Vec<Vec<String>>,
    vk_gamma_2: Vec<Vec<String>>,
    vk_delta_2: Vec<Vec<String>>,
    IC: Vec<Vec<String>>,
}

// Convert string numbers into 64-byte arrays
fn parse_g1_element(g1: &[String]) -> Result<[u8; 64], Box<dyn Error>> {
    let mut bytes = [0u8; 64];
    for (i, num) in g1.iter().enumerate().take(2) { // Only 2 elements needed
        let hex_str = format!("{:064x}", num.parse::<u64>()?);
        let decoded = hex::decode(hex_str)?;
        bytes[i * 32..(i + 1) * 32].copy_from_slice(&decoded);
    }
    Ok(bytes)
}

// Convert G2 elements (128 bytes)
fn parse_g2_element(g2: &[Vec<String>]) -> Result<[u8; 128], Box<dyn Error>> {
    let mut bytes = [0u8; 128];
    for (i, pair) in g2.iter().enumerate().take(2) { // Each element is 2 numbers
        for (j, num) in pair.iter().enumerate().take(2) {
            let hex_str = format!("{:064x}", num.parse::<u64>()?);
            let decoded = hex::decode(hex_str)?;
            bytes[(i * 64) + (j * 32)..(i * 64) + ((j + 1) * 32)].copy_from_slice(&decoded);
        }
    }
    Ok(bytes)
}

// Convert IC points
fn parse_ic_elements(ic: &[Vec<String>]) -> Result<Vec<[u8; 64]>, Box<dyn Error>> {
    let mut ic_elements = Vec::new();
    for g1 in ic.iter() {
        ic_elements.push(parse_g1_element(g1)?);
    }
    Ok(ic_elements)
}
fn hex_to_bytes_64(pair: [&str; 2]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for i in 0..2 {
        let num = BigUint::from_str(pair[i]).expect("Failed to parse number");
        let num_bytes = num.to_bytes_be(); // Arbitrary length, but ≤ 32 bytes expected
        // Compute offset so that the number is right-aligned in its 32-byte slot:
        let offset = 32 - num_bytes.len();
        // Place the number into the correct 32-byte segment:
        bytes[i * 32 + offset..i * 32 + 32].copy_from_slice(&num_bytes);
    }
    bytes
}

fn hex_to_bytes_128(pairs: [[&str; 2]; 2]) -> [u8; 128] {
    let mut bytes = [0u8; 128];
    for i in 0..2 {
        for j in 0..2 {
            let num = BigUint::from_str(pairs[i][j]).expect("Failed to parse number");
            let num_bytes = num.to_bytes_be();
            let offset = 32 - num_bytes.len();
            let start = i * 64 + j * 32 + offset;
            bytes[start..i * 64 + j * 32 + 32].copy_from_slice(&num_bytes);
        }
    }
    bytes
}


// Main function to parse JSON and return verifying key
pub fn parse_verification_key(file_path: &str) -> Result<Groth16Verifyingkey, Box<dyn Error>> {
    let file_content = fs::read_to_string(file_path)?;
    let raw_vk: RawVk = serde_json::from_str(&file_content)?;

    let vk_alpha_g1 = parse_g1_element(&raw_vk.vk_alpha_1)?;
    let vk_beta_g2 = parse_g2_element(&raw_vk.vk_beta_2)?;
    let vk_gamme_g2 = parse_g2_element(&raw_vk.vk_gamma_2)?;
    let vk_delta_g2 = parse_g2_element(&raw_vk.vk_delta_2)?;
    let vk_ic_vec = parse_ic_elements(&raw_vk.IC)?;

    let vk_ic: &'static [[u8; 64]] = Box::leak(Box::new(vk_ic_vec));


    let vk = Groth16Verifyingkey {
        nr_pubinputs: raw_vk.nPublic,
        vk_alpha_g1,
        vk_beta_g2,
        vk_gamme_g2,
        vk_delta_g2,
        vk_ic: &vk_ic,
    };

    Ok(vk)
}


pub const VERIFYINGKEY: Groth16Verifyingkey = Groth16Verifyingkey {
    nr_pubinputs: 2,

    vk_alpha_g1: [
        17, 201, 166, 21, 2, 59, 204, 112, 111, 61, 69, 62, 57, 228, 14, 28, 160, 86, 189, 252, 5, 112,
        240, 171, 172, 102, 97, 14, 180, 204, 20, 93, 5, 246, 135, 28, 199, 232, 163, 53, 150, 143,
        13, 130, 118, 245, 38, 9, 38, 231, 63, 96, 30, 101, 24, 212, 181, 58, 210, 124, 110, 252,
        175, 47,
    ],

    vk_beta_g2: [
        36, 34, 49, 124, 109, 82, 239, 103, 209, 186, 252, 108, 140, 105, 199, 11, 143, 139, 202, 240,
        174, 98, 234, 164, 4, 90, 42, 32, 70, 136, 111, 60, 9, 73, 244, 206, 167, 90, 106, 139,
        120, 126, 164, 104, 49, 252, 70, 129, 107, 151, 125, 148, 235, 34, 146, 215, 247, 28, 84, 41,
        71, 158, 111, 52, 7, 240, 200, 64, 181, 72, 198, 217, 165, 98, 203, 71, 113, 113, 78, 72,
        225, 92, 143, 252, 187, 137, 141, 97, 237, 62, 60, 232, 237, 30, 61, 204, 4, 199, 255, 84,
        30, 201, 205, 104, 68, 188, 91, 189, 111, 69, 218, 216, 53, 98, 130, 179, 102, 189, 225, 111,
        30, 184, 25, 69, 220, 39, 12, 16,
    ],

    vk_gamme_g2: [
        25, 142, 147, 147, 146, 13, 72, 58, 114, 96, 191, 183, 49, 251, 93, 37, 241, 170, 73, 51,
        53, 169, 231, 18, 151, 228, 133, 183, 174, 243, 18, 194, 24, 0, 222, 239, 18, 31, 30, 118,
        66, 106, 0, 102, 94, 92, 68, 121, 103, 67, 34, 212, 247, 94, 218, 221, 70, 222, 189, 92,
        217, 146, 246, 237, 9, 6, 137, 208, 88, 95, 240, 117, 236, 158, 153, 173, 105, 12, 51, 149,
        188, 75, 49, 51, 112, 179, 142, 243, 85, 172, 218, 220, 209, 34, 151, 91, 18, 200, 94, 165,
        219, 140, 109, 235, 74, 171, 113, 128, 141, 203, 64, 143, 227, 209, 231, 105, 12, 67, 211, 123,
        76, 230, 204, 1, 102, 250, 125, 170,
    ],

    vk_delta_g2: [
        45, 184, 86, 194, 65, 151, 229, 53, 143, 240, 182, 39, 103, 16, 160, 84, 146, 27, 196, 2,
        201, 50, 232, 161, 82, 9, 62, 197, 64, 35, 110, 132, 0, 1, 119, 143, 241, 34, 78, 235, 48, 182,
        246, 138, 8, 219, 10, 140, 252, 246, 175, 12, 68, 128, 62, 234, 200, 177, 162, 249, 183, 244,
        24, 241, 43, 47, 236, 205, 127, 19, 98, 39, 214, 74, 213, 230, 35, 163, 156, 176, 254, 39,
        153, 196, 166, 69, 217, 89, 184, 85, 43, 101, 189, 0, 72, 98, 8, 219, 198, 142, 107, 49,
        140, 65, 29, 194, 72, 23, 114, 243, 106, 44, 67, 49, 90, 22, 94, 217, 0, 43, 221, 169, 225, 199,
        238, 147, 133, 195,
    ],

    vk_ic: &[
        [
            37, 122, 193, 181, 242, 172, 193, 133, 143, 135, 182, 224, 169, 216, 91, 185, 161, 22,
            180, 38, 197, 1, 245, 147, 103, 144, 119, 97, 89, 125, 43, 63, 22, 233, 15, 17, 201, 236,
            114, 96, 143, 0, 81, 103, 210, 51, 222, 33, 136, 62, 152, 100, 185, 132, 128, 224, 191, 254,
            26, 196, 164, 18, 157, 251,
        ],
        [
            13, 137, 21, 49, 230, 95, 149, 15, 215, 102, 78, 69, 86, 219, 211, 104, 42, 6, 159, 235,
            110, 241, 215, 43, 8, 220, 101, 226, 247, 140, 149, 76, 4, 126, 13, 231, 30, 51, 172, 11,
            243, 251, 126, 91, 24, 238, 73, 36, 193, 52, 110, 177, 239, 73, 114, 39, 125, 250, 166, 241,
            43, 42, 141, 84,
        ],
    ],
};


pub fn get_hardcoded_vkey()->Groth16Verifyingkey<'static>{
    let vk_ic_vec = vec![
        hex_to_bytes_64(["16952467681835738848271006448256765340790189254246527680117481308791099960127", "10362662040704558484330865932249658384972490156487610402571678601657558670843"]),
        hex_to_bytes_64(["6122271361428918344438247717260514233210581296194523789097526515632779859276", "2031970078125955769684544921336651286368778337230115510384220166253701664084"]),
    ];
    let vk_ic_static: &'static [[u8;64]]= Box::leak(Box::new(vk_ic_vec));

    let vkey = Groth16Verifyingkey{
        nr_pubinputs: 1,
        vk_alpha_g1: hex_to_bytes_64([
            "8045600942223655393026509364940932191342503614322555258630212782313664418909",
            "2697141132539985466920137254617604973800986474029454383039288932987962175279",
        ]),
        vk_beta_g2: hex_to_bytes_128([
            ["4201485070460197654108561280133155826622738656299822383677245601114677473076", "16343676889317591858124870233415853974589780336921477032038392750062967484220"],
            ["2162616173420301100365067531028320648346070123845571008678366419194671991824", "3591615329427085182071417510242364894448428819895517796496426298921981984204"],
        ]),
        vk_gamme_g2: hex_to_bytes_128([
            ["10857046999023057135944570762232829481370756359578518086990519993285655852781", "11559732032986387107991004021392285783925812861821192530917403151452391805634"],
            ["8495653923123431417604973247489272438418190587263600148770280649306958101930", "4082367875863433681332203403145435568316851327593401208105741076214120093531"],
        ]),
        vk_delta_g2: hex_to_bytes_128([
            ["2592035546795762302131886926792352675406369811026157514565592228157462769", "20679776833489472689675778833308791740133199927035400084740036712235008224900"],
            ["400681268123048656161639758001263894286782554390342450468300739449351013827", "19534128653434583021022122433209361431589239205961264088206428247565533202530"],
        ]),
        vk_ic: &vk_ic_static,
        };
    vkey

}

