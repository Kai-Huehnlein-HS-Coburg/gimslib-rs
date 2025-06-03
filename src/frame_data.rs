use std::cell::Cell;

pub struct FrameData<T> {
    current_frame: Cell<usize>,
    frame_datas: Vec<T>,
}

impl<T: Default + Clone> FrameData<T> {
    pub fn new_default(frame_count: usize) -> Self {
        FrameData {
            current_frame: 0.into(),
            frame_datas: vec![T::default(); frame_count],
        }
    }
}

impl<T> FrameData<T> {
    pub fn new(init_data: Vec<T>) -> Self {
        FrameData {
            current_frame: 0.into(),
            frame_datas: init_data,
        }
    }

    pub fn from_fn(frame_count: usize, init_fn: impl FnMut(usize) -> T) -> Self {
        FrameData {
            current_frame: 0.into(),
            frame_datas: (0..frame_count).map(init_fn).collect(),
        }
    }

    pub fn try_from_fn<E>(
        frame_count: usize,
        init_fn: impl FnMut(usize) -> Result<T, E>,
    ) -> Result<Self, E> {
        Ok(FrameData {
            current_frame: 0.into(),
            frame_datas: (0..frame_count).map(init_fn).collect::<Result<_, _>>()?,
        })
    }
}

impl<T> FrameData<T> {
    pub fn increment_frame(&self) {
        self.current_frame
            .set((self.current_frame.get() + 1) % self.frame_datas.len());
    }

    pub fn get_current(&self) -> &T {
        &self.frame_datas[self.current_frame.get()]
    }

    pub fn get_current_mut(&mut self) -> &mut T {
        &mut self.frame_datas[self.current_frame.get()]
    }
}
