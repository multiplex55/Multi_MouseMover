use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

#[derive(Debug)]
pub struct ScreenSnapshot {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    hdc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
}

impl ScreenSnapshot {
    #[cfg(test)]
    pub fn test_bounds(left: i32, top: i32, width: i32, height: i32) -> Self {
        Self {
            left,
            top,
            width,
            height,
            hdc: HDC::default(),
            bitmap: HBITMAP::default(),
            old_bitmap: HGDIOBJ::default(),
        }
    }
}

#[cfg(test)]
fn virtual_to_snapshot_source(
    snapshot_left: i32,
    snapshot_top: i32,
    virtual_x: i32,
    virtual_y: i32,
) -> (i32, i32) {
    (virtual_x - snapshot_left, virtual_y - snapshot_top)
}

impl Drop for ScreenSnapshot {
    fn drop(&mut self) {
        unsafe {
            if !self.hdc.is_invalid() {
                let _ = SelectObject(self.hdc, self.old_bitmap);
                let _ = DeleteDC(self.hdc);
            }
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(self.bitmap.into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::virtual_to_snapshot_source;

    #[test]
    fn source_coordinates_account_for_negative_virtual_origin() {
        assert_eq!(virtual_to_snapshot_source(-1920, -200, -1920, -200), (0, 0));
        assert_eq!(virtual_to_snapshot_source(-1920, -200, 0, 0), (1920, 200));
        assert_eq!(
            virtual_to_snapshot_source(-1920, -200, 640, 480),
            (2560, 680)
        );
    }
}

pub fn capture_virtual_screen() -> Option<ScreenSnapshot> {
    unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        if width <= 0 || height <= 0 {
            return None;
        }

        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return None;
        }

        let memory_dc = CreateCompatibleDC(Some(screen_dc));
        if memory_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            return None;
        }

        let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
        if bitmap.is_invalid() {
            let _ = DeleteDC(memory_dc);
            let _ = ReleaseDC(None, screen_dc);
            return None;
        }

        let old_bitmap = SelectObject(memory_dc, bitmap.into());
        if old_bitmap.is_invalid() {
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory_dc);
            let _ = ReleaseDC(None, screen_dc);
            return None;
        }

        if BitBlt(
            memory_dc,
            0,
            0,
            width,
            height,
            Some(screen_dc),
            left,
            top,
            SRCCOPY,
        )
        .is_err()
        {
            let _ = SelectObject(memory_dc, old_bitmap);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory_dc);
            let _ = ReleaseDC(None, screen_dc);
            return None;
        }

        let _ = ReleaseDC(None, screen_dc);

        Some(ScreenSnapshot {
            left,
            top,
            width,
            height,
            hdc: memory_dc,
            bitmap,
            old_bitmap,
        })
    }
}
