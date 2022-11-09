use std::ops::Deref;

use colorful::pattern_dithering::MatrixSize;
use cust::context::*;
use cust::{error::CudaResult, prelude::*, sys::CUfunction};
use img2ansi::VideoImage;

use crate::PreProcessor;

static PTX: &str = include_str!("../cuda.ptx");

pub struct CudaDitherer {
    ctx: Context,
    stream: Stream,
    frame_size: usize,
    width: u32,
    height: u32,
    multiplier: f32,
    out_buffer: DeviceBuffer<u8>,
    func: FuncHolder,
}

#[ouroboros::self_referencing]
pub struct FuncHolder {
    module: Module,
    #[borrows(module)]
    #[covariant]
    func: Function<'this>,
}

impl CudaDitherer {
    pub fn new(
        width: u32,
        height: u32,
        multiplier: f32,
        matrix_size: MatrixSize,
    ) -> CudaResult<CudaDitherer> {
        let ctx = cust::quick_init()?;
        let module = Module::from_ptx(PTX, &[])?;
        let stream = Stream::new(StreamFlags::NON_BLOCKING, None)?;

        let func_holder = FuncHolderBuilder {
            module,
            func_builder: |module: &Module| {
                match matrix_size {
                    MatrixSize::Eight => module.get_function("dither_8x8"),
                    MatrixSize::Four => module.get_function("dither_4x4"),
                    MatrixSize::Two => module.get_function("dither_2x2"),
                }
                .unwrap()
            },
        }
        .build();

        let frame_size = (width * height) as usize;

        let out_buffer = unsafe { DeviceBuffer::uninitialized(frame_size)? };

        Ok(CudaDitherer {
            ctx,
            func: func_holder,
            stream,
            frame_size,
            width,
            height,
            out_buffer,
            multiplier,
        })
    }

    pub fn dither(&mut self, image: &[u8]) -> CudaResult<Vec<u8>> {
        CurrentContext::set_current(&self.ctx)?;
        let img_gpu = image.deref().as_dbuf().unwrap();
        let func = self.func.borrow_func();
        let (_, block_size) = func.suggested_launch_configuration(0, 0.into())?;
        let grid_size = self.frame_size as u32 / block_size;

        let stream = &mut self.stream;

        unsafe {
            launch!(
                func<<<grid_size, block_size, 0, stream>>>(
                    img_gpu.as_device_ptr(),
                    img_gpu.len(),
                    self.out_buffer.as_device_ptr(),
                    self.width,
                    0.09f32
                )
            )?;
        }

        let mut output = vec![0u8; self.frame_size];

        stream.synchronize()?;

        self.out_buffer.copy_to(&mut output)?;

        Ok(output)
    }
}

impl PreProcessor<crate::video_encoder::DecodedVideoFrame> for CudaDitherer {
    #[inline(always)]
    fn map(&mut self, frame: &mut crate::video_encoder::DecodedVideoFrame) {
        let image = frame.image.as_full_color();
        let data = self.dither(image.deref()).unwrap();

        frame.image = VideoImage::EightBit {
            width: self.width,
            height: self.height,
            data,
        };
    }
}
