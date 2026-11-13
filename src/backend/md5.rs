// MD5 per RFC 1321, computed only as the freedesktop thumbnail cache's filename, where its collision weakness is irrelevant to a cache key; see AGENTS.md "Thumbnail cache".

const BLOCK_BYTES: usize = 64;
const WORD_BYTES: usize = 4;
const BLOCK_WORDS: usize = BLOCK_BYTES / WORD_BYTES;
const DIGEST_BYTES: usize = 16;
const ROUNDS: usize = 64;
const ROUND_GROUP: usize = 16;
const BITS_PER_BYTE: u64 = 8;
const PAD_FIRST_BYTE: u8 = 0x80;
const BIT_LENGTH_BYTES: usize = 8;
const LENGTH_OFFSET: usize = BLOCK_BYTES - BIT_LENGTH_BYTES;

const S: [u32; ROUNDS] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
    5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
    6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

// K[i] is floor(abs(sin(i + 1)) * 2^32), tabulated because this build has no float math at runtime.
const K: [u32; ROUNDS] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

pub fn hex(bytes: &[u8]) -> String {
    let digest = digest(bytes);
    let mut s = String::with_capacity(DIGEST_BYTES * 2);
    for b in digest.iter() {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn digest(bytes: &[u8]) -> [u8; DIGEST_BYTES] {
    let mut msg = bytes.to_vec();
    let bit_len = (bytes.len() as u64).wrapping_mul(BITS_PER_BYTE);
    msg.push(PAD_FIRST_BYTE);
    while msg.len() % BLOCK_BYTES != LENGTH_OFFSET {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    // The four initial state words are RFC 1321 section 3.3's own.
    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;

    for chunk in msg.as_chunks::<BLOCK_BYTES>().0 {
        let mut m = [0u32; BLOCK_WORDS];
        for (i, word) in chunk.as_chunks::<WORD_BYTES>().0.iter().enumerate() {
            m[i] = u32::from_le_bytes(*word);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..ROUNDS {
            // Each group of 16 rounds has its own mixing function and its own message-word schedule, both from RFC 1321 section 3.4.
            let (f, g) = match i / ROUND_GROUP {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % BLOCK_WORDS),
                2 => (b ^ c ^ d, (3 * i + 5) % BLOCK_WORDS),
                _ => (c ^ (b | !d), (7 * i) % BLOCK_WORDS),
            };
            let f = f
                .wrapping_add(a)
                .wrapping_add(K[i])
                .wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut out = [0u8; DIGEST_BYTES];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_1321_vectors() {
        assert_eq!(hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(b"a"), "0cc175b9c0f1b6a831c399e269772661");
        assert_eq!(hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(hex(b"message digest"), "f96b697d7cb7938d525a2f31aaf161d0");
    }

    #[test]
    fn the_real_cache_filename_on_this_box() {
        let uri = "file:///home/gm/Videos/screen-recording-2020-01-02_03-04-05.mp4";
        assert_eq!(hex(uri.as_bytes()), "4b971187c53a6a6ff1925d1147d8dacf");
    }

    #[test]
    fn a_length_that_straddles_the_padding_boundary() {
        // 55, 56 and 64 bytes are the three cases the padding rule gets wrong when it is wrong.
        assert_eq!(hex(&[b'x'; 55]), "04364420e25c512fd958a70738aa8f72");
        assert_eq!(hex(&[b'x'; 56]), "668a72d5ba17f08e62dabcafad6db14b");
        assert_eq!(hex(&[b'x'; 64]), "c1bb4f81d892b2d57947682aeb252456");
    }
}
