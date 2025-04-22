#![allow(unused)]

use raylib::prelude::*;
use v4l::buffer::Type;
use v4l::io::traits::CaptureStream;
use v4l::prelude::*;
use v4l::{
    Device, Format, FourCC, frameinterval::FrameIntervalEnum, framesize::FrameSizeEnum,
    video::Capture,
};

const WIN_WIDTH: i32 = 1280;
const WIN_HEIGHT: i32 = 720;

const CRT_FRAGMENT_SHADER: &'static str = r#"
#version 330
precision lowp float;

varying vec4 fragColor;
varying vec2 fragTexCoord;

uniform sampler2D Texture;

// https://www.shadertoy.com/view/XtlSD7
vec2 CRTCurveUV(vec2 uv)
{
    uv = uv * 2.0 - 1.0;
    vec2 offset = abs( uv.yx ) / vec2( 6.0, 4.0 );
    uv = uv + uv * offset * offset;
    uv = uv * 0.5 + 0.5;
    return uv;
}

void DrawVignette( inout vec3 color, vec2 uv )
{
    float vignette = uv.x * uv.y * ( 1.0 - uv.x ) * ( 1.0 - uv.y );
    vignette = clamp( pow( 16.0 * vignette, 0.3 ), 0.0, 1.0 );
    color *= vignette;
}


void DrawScanline( inout vec3 color, vec2 uv )
{
    float iTime = 0.1;
    float scanline 	= clamp( 0.95 + 0.05 * cos( 3.14 * ( uv.y + 0.008 * iTime ) * 240.0 * 1.0 ), 0.0, 1.0 );
    float grille 	= 0.85 + 0.15 * clamp( 1.5 * cos( 3.14 * uv.x * 640.0 * 1.0 ), 0.0, 1.0 );
    color *= scanline * grille * 1.2;
}

void main() {
    vec2 uv = fragTexCoord;
    vec4 color = fragColor;
    vec2 crtUV = CRTCurveUV(uv);
    vec3 res = texture2D(Texture, uv).rgb * color.rgb;
    if (crtUV.x < 0.0 || crtUV.x > 1.0 || crtUV.y < 0.0 || crtUV.y > 1.0)
    {
        res = vec3(0.0, 0.0, 0.0);
    }
    DrawVignette(res, crtUV);
    DrawScanline(res, uv);
    gl_FragColor = vec4(res, 1.0);

}
"#;

macro_rules! info {
    ($($arg:tt)*) => {
        println!("INFO: {}", format!($($arg)*))
    }
}

fn yuyv_to_rgb(yuyv: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(yuyv.len() * 3 / 2);
    let mut i = 0;
    while i + 3 < yuyv.len() {
        let y0 = yuyv[i] as f32;
        let u = yuyv[i + 1] as f32 - 128.0;
        let y1 = yuyv[i + 2] as f32;
        let v = yuyv[i + 3] as f32 - 128.0;

        for &y in &[y0, y1] {
            let r = (y + 1.402 * v).clamp(0.0, 255.0) as u8;
            let g = (y - 0.344136 * u - 0.714136 * v).clamp(0.0, 255.0) as u8;
            let b = (y + 1.772 * u).clamp(0.0, 255.0) as u8;
            rgb.extend_from_slice(&[r, g, b]);
        }

        i += 4;
    }
    rgb
}

fn get_best_format(dev: &Device) -> (Format, u32) {
    let mut best_width = 0;
    let mut best_height = 0;
    let mut best_fps = 0;
    let mut best_fourcc = FourCC::new(b"YUYV");

    for fmt in dev.enum_formats().unwrap() {
        let fourcc = fmt.fourcc;
        if fourcc.str().expect("fourcc") != "YUYV" {
            continue;
        }

        for size in dev.enum_framesizes(fourcc).unwrap() {
            let (w, h) = match size.size {
                FrameSizeEnum::Discrete(s) => (s.width, s.height),
                _ => continue, // Skip continuous/stepwise
            };

            for interval in dev.enum_frameintervals(fourcc, w, h).unwrap() {
                let fps = match interval.interval {
                    FrameIntervalEnum::Discrete(f) => {
                        if f.numerator > 0 {
                            f.denominator / f.numerator
                        } else {
                            continue;
                        }
                    }
                    _ => continue, // Skip stepwise for simplicity
                };

                if w * h * fps * fps > best_width * best_height * best_fps * best_fps {
                    best_width = w;
                    best_height = h;
                    best_fps = fps;
                    best_fourcc = fourcc;
                }
            }
        }
    }

    (Format::new(best_width, best_height, best_fourcc), best_fps)
}

fn main() {
    let (mut rl, thread) = init()
        .size(WIN_WIDTH, WIN_HEIGHT)
        .title("Shader Cam")
        .log_level(TraceLogLevel::LOG_WARNING)
        .build();

    // camera device
    let mut dev = Device::new(0).expect("Could not open camera");
    let (fmt, fps) = get_best_format(&dev);
    let cam_format = dev.set_format(&fmt).unwrap();

    if cam_format.fourcc.str().expect("fourcc error") != "YUYV" {
        panic!("Unsupported format: {}", cam_format.fourcc);
    }
    info!(
        "Format: {}x{} @ {} FPS ({})",
        cam_format.width, cam_format.height, fps, cam_format.fourcc
    );

    let mut stream = MmapStream::with_buffers(&mut dev, Type::VideoCapture, 4)
        .expect("Failed to create buffer stream");

    let mut img = Image::gen_image_color(
        cam_format.width as i32,
        cam_format.height as i32,
        Color::BLACK,
    );
    img.set_format(PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8);

    let mut shader = rl.load_shader_from_memory(&thread, None, Some(CRT_FRAGMENT_SHADER));
    let mut target = rl
        .load_texture_from_image(&thread, &img)
        .expect("load texture");

    while !rl.window_should_close() {
        let (buf, meta) = stream.next().unwrap();
        let rgb_buf = yuyv_to_rgb(buf);

        let mut d = rl.begin_drawing(&thread);
        d.clear_background(Color::BLACK);

        target.update_texture(&rgb_buf).expect("Update Texture");
        d.draw_shader_mode(&mut shader, |mut s| {
            let src = Rectangle::new(0.0, 0.0, target.width() as f32, target.height() as f32);
            let width = WIN_HEIGHT as f32 * target.width() as f32 / target.height() as f32;
            let x = (WIN_WIDTH as f32 - width) / 2.0;
            let dst = Rectangle::new(x, 0.0, width, WIN_HEIGHT as f32);
            s.draw_texture_pro(&target, src, dst, Vector2::zero(), 0.0, Color::WHITE);
        });
    }
}
