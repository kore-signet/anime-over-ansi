pub trait PreProcessor<V> {
    fn map(&mut self, frame: &mut V);
}

pub struct ProcessorPipeline<V>(Vec<Box<dyn PreProcessor<V>>>);

impl<V> ProcessorPipeline<V> {
    pub fn new() -> ProcessorPipeline<V> {
        ProcessorPipeline(vec![])
    }

    pub fn add<T: PreProcessor<V> + 'static>(&mut self, v: T) {
        self.0.push(Box::new(v));
    }
}

impl<V> PreProcessor<V> for ProcessorPipeline<V> {
    #[inline(always)]
    fn map(&mut self, frame: &mut V) {
        for processor in self.0.iter_mut() {
            processor.map(frame);
        }
    }
}

impl<V> PreProcessor<V> for () {
    fn map(&mut self, _: &mut V) {}
}

pub mod ditherers {
    use colorful::{
        palette::{AnsiColorMap, DistanceMethod},
        pattern_dithering::{self, MatrixSize},
    };
    use image::imageops;

    use img2ansi::VideoImage;

    use super::PreProcessor;

    pub struct FloydSteinberg<T: DistanceMethod>(pub AnsiColorMap<T>);

    impl<T: DistanceMethod> FloydSteinberg<T> {
        pub fn new() -> FloydSteinberg<T> {
            FloydSteinberg(AnsiColorMap::new())
        }
    }

    pub struct Pattern<T: DistanceMethod> {
        pub map: AnsiColorMap<T>,
        pub matrix_size: MatrixSize,
        pub multiplier: f32,
    }

    impl<T: DistanceMethod> Pattern<T> {
        pub fn new(matrix_size: MatrixSize, multiplier: f32) -> Pattern<T> {
            Pattern {
                map: AnsiColorMap::new(),
                matrix_size,
                multiplier,
            }
        }
    }

    impl<T: DistanceMethod> PreProcessor<crate::video_encoder::DecodedVideoFrame>
        for FloydSteinberg<T>
    {
        #[inline(always)]
        fn map(&mut self, frame: &mut crate::video_encoder::DecodedVideoFrame) {
            imageops::dither(frame.image.as_full_color_mut(), &self.0);
        }
    }

    impl<T: DistanceMethod + Send + Sync + Copy>
        PreProcessor<crate::video_encoder::DecodedVideoFrame> for Pattern<T>
    {
        #[inline(always)]
        fn map(&mut self, frame: &mut crate::video_encoder::DecodedVideoFrame) {
            frame.image = VideoImage::EightBit {
                width: frame.image.as_full_color().width(),
                height: frame.image.as_full_color().height(),
                data: pattern_dithering::dither(
                    frame.image.as_full_color(),
                    self.matrix_size,
                    self.multiplier,
                    self.map,
                ),
            };
        }
    }
}
