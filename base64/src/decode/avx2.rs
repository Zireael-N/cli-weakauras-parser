// Based on a research done by Wojciech Muła and Daniel Lemire
// https://arxiv.org/abs/1704.00605
// Copyright (c) 2015-2016, Wojciech Muła, Alfred Klomp, Daniel Lemire
// All rights reserved.
// Licensed under BSD 2-Clause (see LICENSES/fastbase64)

use super::scalar;
#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[cfg(all(test, target_feature = "avx2"))]
#[inline(always)]
/// SAFETY: the caller must ensure that buf can hold AT LEAST (s.len() * 3 / 4) more elements
pub(crate) unsafe fn decode(s: &[u8], buf: &mut Vec<u8>) -> Result<(), &'static str> {
    let mut len = s.len();
    let mut out_len = buf.len();

    let mut ptr = s.as_ptr();
    let mut out_ptr = buf[out_len..].as_mut_ptr();

    unsafe {
        let lut_lo = _mm256_setr_epi8(
            0x15, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x10, 0x10, 0x13, 0x1b, 0x1b, 0x1b,
            0x1b, 0x1b, 0x15, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x10, 0x10, 0x13, 0x1b,
            0x1b, 0x1b, 0x1b, 0x1b,
        );
        let lut_hi = _mm256_setr_epi8(
            0x10, 0x10, 0x01, 0x02, 0x04, 0x08, 0x04, 0x08, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
            0x10, 0x10, 0x10, 0x10, 0x01, 0x02, 0x04, 0x08, 0x04, 0x08, 0x10, 0x10, 0x10, 0x10,
            0x10, 0x10, 0x10, 0x10,
        );
        let lut_roll = _mm256_setr_epi8(
            0, 22, 22, 4, -39, -39, -97, -97, 0, 0, 0, 0, 0, 0, 0, 0, 0, 22, 22, 4, -39, -39, -97,
            -97, 0, 0, 0, 0, 0, 0, 0, 0,
        );

        let mask_lo_nibble = _mm256_set1_epi8(0x0f);

        // Since we'll be writing 32 bytes at a time (last 8 containing zeroes),
        // checking against 43 to make sure the buffer can contain two extra 32-bit words.
        while len >= 43 {
            // Lookup:
            let src = _mm256_loadu_si256(ptr as *const _);
            let hi_nibbles = _mm256_and_si256(_mm256_srli_epi32(src, 4), mask_lo_nibble);
            let lo_nibbles = _mm256_and_si256(src, mask_lo_nibble);
            let lo = _mm256_shuffle_epi8(lut_lo, lo_nibbles);
            let hi = _mm256_shuffle_epi8(lut_hi, hi_nibbles);
            let roll = _mm256_shuffle_epi8(lut_roll, hi_nibbles);

            if _mm256_testz_si256(lo, hi) == 0 {
                return Err("failed to decode base64");
            }

            // Packing:
            let merged =
                _mm256_maddubs_epi16(_mm256_add_epi8(src, roll), _mm256_set1_epi32(0x40014001));
            let swapped = _mm256_madd_epi16(merged, _mm256_set1_epi32(0x10000001));
            let shuffled = _mm256_shuffle_epi8(
                swapped,
                _mm256_setr_epi8(
                    0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14, -1, -1, -1, -1, 0, 1, 2, 4, 5, 6, 8, 9,
                    10, 12, 13, 14, -1, -1, -1, -1,
                ),
            );
            let shuffled =
                _mm256_permutevar8x32_epi32(shuffled, _mm256_setr_epi32(0, 1, 2, 4, 5, 6, -1, -1));
            _mm256_storeu_si256(out_ptr as *mut _, shuffled);
            out_ptr = out_ptr.add(24);
            out_len += 24;

            len -= 32;
            ptr = ptr.add(32);
        }
        buf.set_len(out_len);

        scalar::decode(core::slice::from_raw_parts(ptr, len), buf)
    }
}
