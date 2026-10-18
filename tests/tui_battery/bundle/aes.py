"""AES-128 block encryption in ECB mode, for the fixture's password-protected document only."""


def _tables():
    sbox, p, q = [0] * 256, 1, 1
    while True:
        # p walks the multiplicative group by 3, q by its inverse; the affine map of q gives the S-box entry for p.
        p = p ^ ((p << 1) & 0xFF) ^ (0x1B if p & 0x80 else 0)
        q ^= (q << 1) & 0xFF
        q ^= (q << 2) & 0xFF
        q ^= (q << 4) & 0xFF
        if q & 0x80:
            q ^= 0x09
        x = q ^ ((q << 1) | (q >> 7)) & 0xFF ^ ((q << 2) | (q >> 6)) & 0xFF ^ ((q << 3) | (q >> 5)) & 0xFF ^ ((q << 4) | (q >> 4)) & 0xFF
        sbox[p] = (x ^ 0x63) & 0xFF
        if p == 1:
            break
    sbox[0] = 0x63
    return sbox


SBOX = _tables()


def xtime(b):
    return ((b << 1) ^ 0x1B) & 0xFF if b & 0x80 else b << 1


def expand(key):
    words, rcon = [list(key[i:i + 4]) for i in range(0, 16, 4)], 1
    for i in range(4, 44):
        w = list(words[i - 1])
        if i % 4 == 0:
            w = [SBOX[b] for b in w[1:] + w[:1]]
            w[0] ^= rcon
            rcon = xtime(rcon)
        words.append([a ^ b for a, b in zip(words[i - 4], w)])
    return [sum(words[r * 4:r * 4 + 4], []) for r in range(11)]


def block(keys, data):
    s = [b ^ k for b, k in zip(data, keys[0])]
    for r in range(1, 11):
        s = [SBOX[b] for b in s]
        s = [s[(i + 4 * (i % 4)) % 16] for i in range(16)]
        if r < 10:
            out = []
            for c in range(4):
                a = s[c * 4:c * 4 + 4]
                t = a[0] ^ a[1] ^ a[2] ^ a[3]
                out += [a[i] ^ t ^ xtime(a[i] ^ a[(i + 1) % 4]) for i in range(4)]
            s = out
        s = [b ^ k for b, k in zip(s, keys[r])]
    return bytes(s)


def ecb(key, data):
    if len(key) != 16 or len(data) % 16:
        raise ValueError("AES-128 ECB needs a 16 byte key and whole blocks")
    keys = expand(key)
    return b"".join(block(keys, data[i:i + 16]) for i in range(0, len(data), 16))
