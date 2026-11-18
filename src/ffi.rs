//! PDFium public C ABI. All handles stay on the thread that owns the document.
#![allow(non_snake_case)]
use std::ffi::{c_char, c_void};
pub type H = *mut c_void;
#[repr(C)]
#[derive(Default, Clone, Copy, Debug)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}
#[repr(C)]
pub struct Writer {
    pub version: i32,
    pub write: unsafe extern "C" fn(*mut Writer, *const u8, u32) -> i32,
    pub sink: *mut c_void,
}

#[link(name = "pdfium", kind = "raw-dylib")]
extern "system" {
    pub fn FPDF_InitLibrary();
    pub fn FPDF_GetLastError() -> u32;
    pub fn FPDF_CreateNewDocument() -> H;
    pub fn FPDF_LoadMemDocument64(data: *const u8, size: usize, password: *const c_char) -> H;
    pub fn FPDF_CloseDocument(doc: H);
    pub fn FPDF_GetDocPermissions(doc: H) -> u32;
    pub fn FPDF_GetSecurityHandlerRevision(doc: H) -> i32;
    pub fn FPDF_GetPageCount(doc: H) -> i32;
    pub fn FPDF_LoadPage(doc: H, index: i32) -> H;
    pub fn FPDF_ClosePage(page: H);
    pub fn FPDF_GetPageWidthF(page: H) -> f32;
    pub fn FPDF_GetPageHeightF(page: H) -> f32;
    pub fn FPDF_PageToDevice(
        page: H,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        r: i32,
        px: f64,
        py: f64,
        dx: *mut i32,
        dy: *mut i32,
    ) -> i32;
    pub fn FPDF_DeviceToPage(
        page: H,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        r: i32,
        dx: i32,
        dy: i32,
        px: *mut f64,
        py: *mut f64,
    ) -> i32;
    pub fn FPDFBitmap_CreateEx(w: i32, h: i32, format: i32, data: *mut u8, stride: i32) -> H;
    pub fn FPDFBitmap_Destroy(bmp: H);
    pub fn FPDF_RenderPageBitmap(
        bmp: H,
        page: H,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        rotate: i32,
        flags: i32,
    );
    pub fn FPDF_SaveAsCopy(doc: H, writer: *mut Writer, flags: u32) -> i32;
    pub fn FPDF_ImportPagesByIndex(dst: H, src: H, indices: *const i32, count: u32, at: i32)
        -> i32;
    pub fn FPDF_MovePages(doc: H, indices: *const i32, count: u32, index: i32) -> i32;
    pub fn FPDFPage_New(doc: H, index: i32, w: f64, h: f64) -> H;
    pub fn FPDFPage_Delete(doc: H, index: i32);
    pub fn FPDFPage_GetRotation(page: H) -> i32;
    pub fn FPDFPage_SetRotation(page: H, rotation: i32);
    pub fn FPDFPage_GenerateContent(page: H) -> i32;
    pub fn FPDFPage_CountObjects(page: H) -> i32;
    pub fn FPDFPage_GetObject(page: H, index: i32) -> H;
    pub fn FPDFPage_RemoveObject(page: H, obj: H) -> i32;
    pub fn FPDFPage_InsertObject(page: H, obj: H);
    pub fn FPDFPage_InsertObjectAtIndex(page: H, obj: H, index: usize) -> i32;
    pub fn FPDFPageObj_Destroy(obj: H);
    pub fn FPDFPageObj_GetType(obj: H) -> i32;
    pub fn FPDFPath_CountSegments(obj: H) -> i32;
    pub fn FPDFPath_GetPathSegment(obj: H, index: i32) -> H;
    pub fn FPDFPathSegment_GetPoint(segment: H, x: *mut f32, y: *mut f32) -> i32;
    pub fn FPDFPathSegment_GetType(segment: H) -> i32;
    pub fn FPDFPathSegment_GetClose(segment: H) -> i32;
    pub fn FPDFPageObj_GetMatrix(obj: H, m: *mut Matrix) -> i32;
    pub fn FPDFPageObj_SetMatrix(obj: H, m: *const Matrix) -> i32;
    pub fn FPDFPageObj_GetBounds(obj: H, l: *mut f32, b: *mut f32, r: *mut f32, t: *mut f32)
        -> i32;
    pub fn FPDFPageObj_GetFillColor(
        obj: H,
        r: *mut u32,
        g: *mut u32,
        b: *mut u32,
        a: *mut u32,
    ) -> i32;
    pub fn FPDFPageObj_SetFillColor(obj: H, r: u32, g: u32, b: u32, a: u32) -> i32;
    pub fn FPDFPageObj_SetIsActive(obj: H, state: i32) -> i32;
    pub fn FPDFPageObj_GetClipPath(obj: H) -> H;
    pub fn FPDFClipPath_CountPaths(path: H) -> i32;
    pub fn FPDFPageObj_NewImageObj(doc: H) -> H;
    pub fn FPDFImageObj_SetBitmap(pages: *mut H, count: i32, obj: H, bmp: H) -> i32;
    pub fn FPDFText_LoadPage(page: H) -> H;
    pub fn FPDFText_ClosePage(text: H);
    pub fn FPDFText_CountChars(text: H) -> i32;
    pub fn FPDFText_GetUnicode(text: H, index: i32) -> u32;
    pub fn FPDFText_GetCharBox(
        text: H,
        index: i32,
        l: *mut f64,
        r: *mut f64,
        b: *mut f64,
        t: *mut f64,
    ) -> i32;
    pub fn FPDFTextObj_GetText(obj: H, text: H, out: *mut u16, size: u32) -> u32;
    pub fn FPDFTextObj_GetFont(obj: H) -> H;
    pub fn FPDFTextObj_GetFontSize(obj: H, sz: *mut f32) -> i32;
    pub fn FPDFTextObj_GetTextRenderMode(obj: H) -> i32;
    pub fn FPDFTextObj_SetFontSize(obj: H, sz: f32) -> i32;
    pub fn FPDFFont_GetFamilyName(font: H, buf: *mut u8, size: usize) -> usize;
    pub fn FPDFFont_GetFontData(font: H, buf: *mut u8, size: usize, out: *mut usize) -> i32;
    pub fn FPDFFont_GetWeight(font: H) -> i32;
    pub fn FPDFFont_GetItalicAngle(font: H, out: *mut i32) -> i32;
    pub fn FPDFText_SetText(obj: H, text: *const u16) -> i32;
    pub fn FPDFText_LoadFont(doc: H, data: *const u8, size: u32, kind: i32, cid: i32) -> H;
    pub fn FPDFFont_Close(font: H);
    pub fn FPDFPageObj_CreateTextObj(doc: H, font: H, sz: f32) -> H;
    pub fn FPDFBookmark_GetFirstChild(doc: H, parent: H) -> H;
    pub fn FPDFBookmark_GetNextSibling(doc: H, bookmark: H) -> H;
    pub fn FPDFBookmark_GetTitle(bookmark: H, buf: *mut u16, size: u32) -> u32;
    pub fn FPDFBookmark_GetDest(doc: H, bookmark: H) -> H;
    pub fn FPDFBookmark_GetAction(bookmark: H) -> H;
    pub fn FPDFAction_GetType(action: H) -> u32;
    pub fn FPDFAction_GetDest(doc: H, action: H) -> H;
    pub fn FPDFDest_GetDestPageIndex(doc: H, dest: H) -> i32;
    pub fn FPDFDest_GetLocationInPage(
        dest: H,
        hx: *mut i32,
        hy: *mut i32,
        hz: *mut i32,
        x: *mut f32,
        y: *mut f32,
        z: *mut f32,
    ) -> i32;
    pub fn FPDFLink_GetLinkAtPoint(page: H, x: f64, y: f64) -> H;
    pub fn FPDFLink_GetDest(doc: H, link: H) -> H;
    pub fn FPDFLink_GetAction(link: H) -> H;
    pub fn FPDF_GetPageLabel(doc: H, page: i32, buf: *mut u16, size: u32) -> u32;
}
