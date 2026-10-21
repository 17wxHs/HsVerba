//! OCR 探针：ocr_probe <图片路径> [语言标签]
use verba_lib::ocr;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let langs = ocr::available_languages();
    println!("LANGS {:?}", langs);
    if a.len() < 2 {
        println!("usage: ocr_probe <image> [lang]");
        return;
    }
    let img = image::open(&a[1]).expect("open image");
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let bgra: Vec<u8> = rgba
        .pixels()
        .flat_map(|p| [p[2], p[1], p[0], 255u8])
        .collect();
    let hint = a.get(2).map(|s| s.as_str());
    match ocr::recognize(&bgra, w as i32, h as i32, hint) {
        Ok(lines) => {
            println!("LINES {}", lines.len());
            for l in &lines {
                println!("[{:.0},{:.0},{:.0},{:.0}] {}", l.x, l.y, l.w, l.h, l.text);
            }
            println!("--- 段落实用版 ---");
            println!("{}", ocr::to_paragraphs(&lines));
        }
        Err(e) => println!("ERR {}", e),
    }
}
