//! Conservative diagnostic flags, not a substitute for reviewing transition frames.
pub fn white_block(pixels: &[u8], width: usize, dpi: u32) -> bool {
    if width == 0 {
        return false;
    }
    let min_width = (160 * dpi as usize / 96).max(1);
    let min_height = (20 * dpi as usize / 96).max(1);
    let mut rows = 0;
    for row in pixels.chunks_exact(width * 4) {
        let mut run = 0;
        let mut broad = false;
        for p in row.chunks_exact(4) {
            run = if p[..3].iter().all(|v| *v > 235) {
                run + 1
            } else {
                0
            };
            broad |= run >= min_width;
        }
        rows = if broad { rows + 1 } else { 0 };
        if rows >= min_height {
            return true;
        }
    }
    false
}
pub fn differs(a: &[u8], b: &[u8]) -> bool {
    a.len() != b.len()
        || a.chunks_exact(4)
            .zip(b.chunks_exact(4))
            .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(a, b)| a.abs_diff(*b) > 12))
            .count()
            * 200
            > a.len() / 4
}

pub fn content_differs(a: &[u8], b: &[u8], width: usize, height: usize, dpi: u32) -> bool {
    if a.len() != b.len() || width == 0 || a.len() != width * height * 4 {
        return true;
    }
    let top = (70 * dpi as usize / 96).min(height);
    let bottom = height.saturating_sub(70 * dpi as usize / 96).max(top);
    differs(
        &a[top * width * 4..bottom * width * 4],
        &b[top * width * 4..bottom * width * 4],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn static_content_ignores_navigation_but_detects_partial_form() {
        let old = vec![0; 200 * 200 * 4];
        let mut next = old.clone();
        next[..200 * 30 * 4].fill(255);
        assert!(!content_differs(&old, &next, 200, 200, 96));
        next[200 * 90 * 4..200 * 100 * 4].fill(255);
        assert!(content_differs(&old, &next, 200, 200, 96));
    }
    #[test]
    fn broad_flash_is_detected_but_small_text_is_not() {
        let dark = vec![0; 200 * 40 * 4];
        assert!(!white_block(&dark, 200, 96));
        let mut flash = dark.clone();
        for y in 0..20 {
            for x in 0..160 {
                flash[(y * 200 + x) * 4..(y * 200 + x) * 4 + 3].fill(255);
            }
        }
        assert!(white_block(&flash, 200, 96));
        assert!(!white_block(&flash, 200, 192));
    }
    #[test]
    fn mixed_frame_differs_from_both_endpoints() {
        let old = vec![0; 400];
        let new = vec![100; 400];
        let mut mixed = old.clone();
        mixed[200..].copy_from_slice(&new[200..]);
        assert!(differs(&mixed, &old) && differs(&mixed, &new));
        assert!(!differs(&old, &old));
        let mut alpha = old.clone();
        for p in alpha.chunks_exact_mut(4) {
            p[3] = 255;
        }
        assert!(!differs(&old, &alpha));
    }
}
