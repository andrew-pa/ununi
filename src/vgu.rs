#![allow(dead_code)]
use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::shared::winerror::*;
use winapi::um::winuser::*;
use winapi::Interface;
use winapi::um::dcommon::{D2D_SIZE_U, D2D1_PIXEL_FORMAT, D2D1_ALPHA_MODE_PREMULTIPLIED };
use winapi::um::d2d1::*;
use winapi::um::dwrite::*;
use winapi::um::errhandlingapi::*;
use winapi::um::libloaderapi::*;
use winapi::um::unknwnbase::*;
use winapi::shared::guiddef::*;

use std::fmt;
use std::ops;
use std::error::Error;
use std::ptr::{null_mut, null};
use std::mem::{size_of, MaybeUninit, transmute};

pub struct HResultError {
    res: HRESULT
}

impl HResultError {
    pub fn new(hr: HRESULT) -> HResultError { HResultError { res: hr } }
    pub fn last_win32_error() -> HResultError {
        unsafe {
            HResultError { res: GetLastError() as i32 }
        }
    }
}

impl Error for HResultError {
    fn description(&self) -> &str { "Windows error" }
}

impl fmt::Debug for HResultError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "HRESULT 0x{:x} {:?}", self.res, self.res)
    }
}

impl fmt::Display for HResultError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "HRESULT 0x{:x} {:?}", self.res, self.res)
    }
}

pub trait IntoResult<E> {
    fn into_result<T, F: FnOnce() -> T>(self, f: F) -> Result<T, E>; 
    fn ok(self) -> Result<(), HResultError>;
}

impl IntoResult<HResultError> for HRESULT {
    fn into_result<T, F: FnOnce() -> T>(self, f: F) -> Result<T, HResultError> {
        match self {
            S_OK => Ok(f()),
            v => Err(HResultError::new(v))
        }
    }
    fn ok(self) -> Result<(), HResultError> {
        match self {
            S_OK => Ok(()),
            v => Err(HResultError::new(v))
        }
    }
}

/*impl RECT {
    fn zero() -> Rect { Rect { 0,0,0,0 } }
    fn extents(width: i32, height: i32) -> Rect { Rect { 0,width,0,height } }
    fn new(x: i32, y: i32, w: i32, h: i32) -> Rect { Rect { x, x+w, y, y+h } }
}*/

extern "system" {
    pub fn SetProcessDpiAwareness(value: DWORD) -> HRESULT;
}

pub struct Window {
    pub hndl: HWND
}

impl Window {
    
    pub fn from_handle(hndl: HWND) -> Window { Window { hndl } }
    pub fn new(size: (i32, i32), prc: WNDPROC) -> Result<Window, HResultError> {
        unsafe {
            let module = GetModuleHandleW(null());
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as UINT,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: prc,
                cbClsExtra: 0, cbWndExtra: 32,
                hInstance: module,
                hIcon: null_mut(),
                hCursor: LoadCursorW(module, IDC_ARROW),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: &[65u16,0u16] as *const u16,
                hIconSm: null_mut()
            };
            if RegisterClassExW(&class) == 0 {
                return Err(HResultError::last_win32_error())
            }
            let hwnd = CreateWindowExW(
                WS_EX_COMPOSITED, //assuming we're going to use this with DirectX
                &[65u16,0u16] as *const u16,
                &[65u16,0u16] as *const u16,
                WS_POPUP,
                300, 300,
                size.0, size.1,
                null_mut(), 
                null_mut(), 
                module, 
                null_mut());
            if hwnd.is_null() {
                Err(HResultError::last_win32_error())
            } else {
                Ok(Window::from_handle(hwnd))
            }
        }
    }
    pub fn client_rect(&self) -> RECT {
        let mut rc: RECT = RECT{left:0,right:0,bottom:0,top:0};
        unsafe { GetClientRect(self.hndl, &mut rc); }
        rc
    }

    pub fn message_loop() {
        unsafe {
            let mut msg: MaybeUninit<MSG> = MaybeUninit::uninit();
            while GetMessageW(msg.as_mut_ptr(), null_mut(), 0, 0) != 0 {
                let msg = msg.assume_init();
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            UnregisterClassW(&[65u16,0u16] as *const u16, GetModuleHandleW(null()));
            CloseWindow(self.hndl);
        }
        self.hndl = null_mut();
    }
}

pub struct Com<T> {
    pub punk: *mut IUnknown,
    pub p: *mut T
}

impl<T> Com<T> {
    pub fn from_ptr(p: *mut T) -> Com<T> {
        Com { punk: p as *mut IUnknown, p: p }
    }

    pub fn query_interface<U>(&self, id: IID) -> Result<Com<U>, HResultError> {
        unsafe {
            let mut up: MaybeUninit<*mut U> = MaybeUninit::uninit();
            (*self.punk).QueryInterface(&id, up.as_mut_ptr() as *mut *mut winapi::ctypes::c_void).into_result(|| Com { punk: self.punk, p: up.assume_init() })
        }
    }
}

impl<T> Clone for Com<T> {
    fn clone(&self) -> Self {
        unsafe { (*self.punk).AddRef(); }
        Com { punk: self.punk, p: self.punk as *mut T }
    }

    fn clone_from(&mut self, source: &Self) {
        unsafe { (*self.punk).Release(); }
        self.punk = source.punk;
        unsafe { (*self.punk).AddRef(); }
    }
}

impl<T> Drop for Com<T> {
    fn drop(&mut self) {
        if self.p != null_mut() {
            unsafe { (*self.punk).Release(); }
        }
        self.p = null_mut();
    }
}

impl<T> Into<*mut T> for Com<T> {
    fn into(self) -> *mut T {
        self.p
    }
}

impl<T> ops::Deref for Com<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.p }
    }
}
impl<T> ops::DerefMut for Com<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.p }
    }
}



pub type Factory = Com<ID2D1Factory>;

impl Factory {
    pub fn new() -> Result<Com<ID2D1Factory>, HResultError> {
        let null_opts: *const D2D1_FACTORY_OPTIONS = null();
        let mut fac: MaybeUninit<*mut ID2D1Factory> = MaybeUninit::uninit();
        unsafe {
            D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, &ID2D1Factory::uuidof(), null_opts, transmute(fac.as_mut_ptr()))
                .into_result(|| Com::from_ptr(fac.assume_init()))
        }
    }
}
/*
pub type ColorF = D2D1_COLOR_F;
impl ColorF {
    fn rgba(r: f32, g: f32, b: f32, a: f32) -> ColorF {
        D2D1_COLOR_F {r,g,b,a}
    }
}

pub type RectF = D2D1_RECT_F;
impl RectF {
    fn xywh(x: f32, y: f32, w: f32, h: f32) -> RectF {
        D2D1_RECT_F { left:x, right:x+w, top:y, bottom:y+h }
    }
    fn lrtb(l: f32, r: f32, t: f32, b: f32) -> RectF {
        D2D1_RECT_F { left:l, right:r, top:t, bottom:b }
    }
}
*/
pub type Brush = Com<ID2D1Brush>;
pub type Font = Com<IDWriteTextFormat>;
pub type TextLayout = Com<IDWriteTextLayout>;

pub type WindowRenderTarget = Com<ID2D1HwndRenderTarget>;

impl WindowRenderTarget {
    pub fn new(fct: Factory, win: &Window) -> Result<WindowRenderTarget, HResultError> {
        let rc = win.client_rect();
        let size = D2D_SIZE_U { width: (rc.right-rc.left) as u32, height: (rc.bottom-rc.top) as u32 };
        let pxfmt = D2D1_PIXEL_FORMAT {
            format: winapi::shared::dxgiformat::DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED
        };
        let render_props = D2D1_RENDER_TARGET_PROPERTIES {
            _type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: pxfmt,
            dpiX: 0.0, dpiY: 0.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let hwnd_rp = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd: win.hndl,
            pixelSize: size,
            presentOptions: D2D1_PRESENT_OPTIONS_NONE
        };

        let mut hrt: *mut ID2D1HwndRenderTarget = null_mut();
        unsafe {
            fct.CreateHwndRenderTarget(&render_props, &hwnd_rp, &mut hrt).into_result(|| Com::from_ptr(transmute(hrt)))
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        let rs = D2D_SIZE_U { width: w, height: h };
        unsafe { self.Resize(&rs); }
    }
}


impl Brush {
    pub fn solid_color(rt: WindowRenderTarget, col: D2D1_COLOR_F) -> Result<Brush, HResultError> {
        unsafe {
            let mut brsh: *mut ID2D1SolidColorBrush = null_mut();
            rt.CreateSolidColorBrush(&col, null_mut(), &mut brsh).into_result(|| Com::from_ptr(transmute(brsh)))
        }
    }
}

pub type TextFactory = Com<IDWriteFactory>;

impl TextFactory {
    pub fn new() -> Result<TextFactory, HResultError> {
        unsafe {
            let mut fac : MaybeUninit<*mut IDWriteFactory> = MaybeUninit::uninit();
            DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, &IDWriteFactory::uuidof(), transmute(fac.as_mut_ptr())).into_result(|| Com::from_ptr(fac.assume_init()))
        }
    }
}


impl Font {
    pub fn new(fac: TextFactory, name: String, weight: DWRITE_FONT_WEIGHT, style: DWRITE_FONT_STYLE, size: f32) -> Result<Font, HResultError> {
        unsafe {
            let mut txf: MaybeUninit<*mut IDWriteTextFormat> = MaybeUninit::uninit();
            let mut wname = name.encode_utf16().collect::<Vec<u16>>();
            wname.push(0u16);
            wname.push(0u16);
            fac.CreateTextFormat(wname.as_ptr(), null_mut(), 
                                 weight, style, DWRITE_FONT_STRETCH_NORMAL, size, 
                                 [0u16].as_ptr(), txf.as_mut_ptr()).into_result(|| Com::from_ptr(txf.assume_init()))
        }
    }
}

impl TextLayout {
    pub fn new(fac: TextFactory, text: &str, f: &Font, width: f32, height: f32) -> Result<TextLayout, Box<dyn Error>> {
        unsafe {
            let mut lo: MaybeUninit<*mut IDWriteTextLayout> = MaybeUninit::uninit();
            let mut txd = text.encode_utf16().collect::<Vec<u16>>();
            txd.push(0u16);
            txd.push(0u16);
            fac.CreateTextLayout(txd.as_ptr(), txd.len() as u32, f.p, width, height, lo.as_mut_ptr())
                .into_result(|| Com::from_ptr(lo.assume_init())).map_err(Into::into)
        }
    }
    pub fn bounds(&self) -> D2D1_RECT_F {
        unsafe {
            let mut metrics: MaybeUninit<DWRITE_TEXT_METRICS> = MaybeUninit::uninit();
            (*self.p).GetMetrics(metrics.as_mut_ptr());
            let metrics = metrics.assume_init();
            D2D1_RECT_F { left: metrics.left, top: metrics.top, right: metrics.left+metrics.width, bottom: metrics.top+metrics.height }
        }
    }
    pub fn char_bounds(&self, index: usize) -> D2D1_RECT_F {
        unsafe {
            let mut ht: MaybeUninit<DWRITE_HIT_TEST_METRICS> = MaybeUninit::uninit();
            let (mut x, mut y) = (0.0, 0.0);
            (*self.p).HitTestTextPosition(index as u32, 0, &mut x, &mut y, ht.as_mut_ptr());
            let ht = ht.assume_init();
            D2D1_RECT_F { left: x, top: y, right: x+ht.width, bottom: y+ht.height }
        }
    }
}

#[cfg(test)]
mod tests {

    use ::vgu::*;
    
    
    

    #[test]
    #[ignore] //mutex with create_d2d_window
    fn create_window() {
        let _win = ::vgu::Window::new((200,200), Some(DefWindowProcW)).expect("creating Win32 window");
    }

    #[test]
    fn create_d2d_factory() {
        let _fac = ::vgu::Factory::new().expect("creating Direct2D factory");
    }

    #[test]
    fn create_d2d_window() {
        let win = Window::new((200,200), Some(DefWindowProcW)).expect("creating Win32 window");
        let fac = Factory::new().expect("creating Direct2D factory");
        let rt = WindowRenderTarget::new(fac, &win).expect("creating HwndRenderTarget");
        let _bc = Brush::solid_color(rt, D2D1_COLOR_F { r: 0.8, g: 0.5, b: 0.1, a: 1.0 }).expect("creating Solid Color Brush");
    }

    #[test]
    fn create_dwrite_factory() {
        let _fac = TextFactory::new().expect("creating DWrite factory");
    }

    #[test]
    fn create_dwrite_font() {
        let fac = TextFactory::new().expect("creating DWrite factory");
        let _fnt = Font::new(fac.clone(), String::from("Arial"), DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, 64.0).expect("creating Arial font");
    }
}


