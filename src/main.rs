#![allow(unused)]

use macroquad::prelude::*;
use nokhwa::{Camera, pixel_format::*, utils::*};

const WIN_WIDTH: i32 = 1280;
const WIN_HEIGHT: i32 = 720;

const CRT_VERTEX_SHADER: &'static str = "#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

varying lowp vec2 uv;
varying lowp vec4 color;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}
";

const CRT_FRAGMENT_SHADER: &'static str = r#"
#version 100
precision lowp float;

varying vec4 color;
varying vec2 uv;

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

fn window_conf() -> Conf {
    Conf {
        window_title: "Shader cam".to_string(),
        window_width: WIN_WIDTH,
        window_height: WIN_HEIGHT,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let format = RequestedFormat::new::<RgbAFormat>(RequestedFormatType::None);

    let mut cam = Camera::new(CameraIndex::Index(0), format).unwrap();
    cam.open_stream().unwrap();

    let res = cam.resolution();
    let mut tex = Texture2D::from_image(&Image {
        bytes: vec![0; (res.width() * res.height() * 4) as usize], // dummy data
        width: res.width() as u16,
        height: res.height() as u16,
    });
    tex.set_filter(FilterMode::Nearest);

    let material = load_material(
        ShaderSource::Glsl {
            vertex: CRT_VERTEX_SHADER,
            fragment: CRT_FRAGMENT_SHADER,
        },
        Default::default(),
    )
    .unwrap();

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        let frame = cam.frame().unwrap();
        let res = frame.resolution();
        let rgba = frame.decode_image::<RgbAFormat>().unwrap();

        tex.update(&Image {
            bytes: rgba.to_vec(),
            width: res.width() as u16,
            height: res.height() as u16,
        });

        gl_use_material(&material);
        clear_background(BLACK);
        draw_texture(&tex, 0.0, 0.0, WHITE);
        gl_use_default_material();

        next_frame().await;
    }
}
