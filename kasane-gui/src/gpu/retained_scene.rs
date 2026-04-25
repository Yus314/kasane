//! Retained-mode scene: frame-to-frame diff detection for partial GPU upload.
//!
//! `RetainedScene` stores the previous frame's quad instance data and compares
//! it against the current frame to produce `DiffOp`s. Only changed ranges are
//! uploaded to the GPU, reducing bandwidth for mostly-static scenes (e.g.,
//! only the cursor line changed).

/// A range of instances that changed between frames.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffOp {
    /// Start index (in floats) within the instance buffer.
    pub offset: usize,
    /// Length (in floats) of the changed region.
    pub len: usize,
}

/// Retained scene state for frame-to-frame diffing.
pub struct RetainedScene {
    /// Previous frame's instance data (floats).
    prev: Vec<f32>,
    /// Number of floats per instance.
    stride: usize,
    /// Whether the previous frame data is valid for diffing.
    valid: bool,
}

impl RetainedScene {
    /// Create a new retained scene with the given instance stride (in floats).
    pub fn new(stride: usize) -> Self {
        Self {
            prev: Vec::new(),
            stride,
            valid: false,
        }
    }

    /// Invalidate the retained state (forces full upload next frame).
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// Compare the current frame's data against the previous frame.
    ///
    /// Returns a list of `DiffOp`s describing which ranges changed, plus
    /// a boolean indicating whether a full upload is needed (length changed
    /// or first frame).
    ///
    /// After calling this, the current data is stored as the new "previous".
    pub fn diff(&mut self, current: &[f32]) -> (Vec<DiffOp>, bool) {
        if !self.valid || self.prev.len() != current.len() {
            // Full upload needed: size changed or first frame
            self.prev.clear();
            self.prev.extend_from_slice(current);
            self.valid = true;
            return (Vec::new(), true);
        }

        let ops = self.compute_diff_ops(current);
        // Update stored data for changed regions only
        for op in &ops {
            self.prev[op.offset..op.offset + op.len]
                .copy_from_slice(&current[op.offset..op.offset + op.len]);
        }
        (ops, false)
    }

    /// Compute changed instance ranges by comparing float-by-float.
    ///
    /// Adjacent changed instances are merged into contiguous ranges to
    /// minimize the number of `write_buffer` calls.
    fn compute_diff_ops(&self, current: &[f32]) -> Vec<DiffOp> {
        let stride = self.stride;
        let instance_count = current.len() / stride;
        let mut ops = Vec::new();
        let mut i = 0;

        while i < instance_count {
            let base = i * stride;
            let end = base + stride;

            if self.prev[base..end] != current[base..end] {
                // Found a changed instance — extend to cover adjacent changes
                let start = base;
                i += 1;
                while i < instance_count {
                    let b = i * stride;
                    let e = b + stride;
                    if self.prev[b..e] != current[b..e] {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let merged_end = i * stride;
                ops.push(DiffOp {
                    offset: start,
                    len: merged_end - start,
                });
            } else {
                i += 1;
            }
        }

        ops
    }

    /// Number of instances in the previous frame.
    pub fn prev_instance_count(&self) -> usize {
        if self.stride == 0 {
            0
        } else {
            self.prev.len() / self.stride
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_is_full_upload() {
        let mut scene = RetainedScene::new(4);
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let (ops, full) = scene.diff(&data);
        assert!(full);
        assert!(ops.is_empty());
    }

    #[test]
    fn identical_frames_no_diff() {
        let mut scene = RetainedScene::new(4);
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        scene.diff(&data);
        let (ops, full) = scene.diff(&data);
        assert!(!full);
        assert!(ops.is_empty());
    }

    #[test]
    fn single_instance_changed() {
        let mut scene = RetainedScene::new(4);
        let data1 = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        scene.diff(&data1);

        let data2 = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 99.0, 8.0];
        let (ops, full) = scene.diff(&data2);
        assert!(!full);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0], DiffOp { offset: 4, len: 4 });
    }

    #[test]
    fn adjacent_changes_merged() {
        let mut scene = RetainedScene::new(2);
        let data1 = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        scene.diff(&data1);

        let mut data2 = data1.clone();
        data2[2] = 99.0; // instance 1
        data2[4] = 99.0; // instance 2
        let (ops, full) = scene.diff(&data2);
        assert!(!full);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0], DiffOp { offset: 2, len: 4 });
    }

    #[test]
    fn non_adjacent_changes_separate() {
        let mut scene = RetainedScene::new(2);
        let data1 = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        scene.diff(&data1);

        let mut data2 = data1.clone();
        data2[0] = 99.0; // instance 0
        data2[6] = 99.0; // instance 3
        let (ops, full) = scene.diff(&data2);
        assert!(!full);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0], DiffOp { offset: 0, len: 2 });
        assert_eq!(ops[1], DiffOp { offset: 6, len: 2 });
    }

    #[test]
    fn size_change_forces_full_upload() {
        let mut scene = RetainedScene::new(4);
        let data1 = vec![1.0, 2.0, 3.0, 4.0];
        scene.diff(&data1);

        let data2 = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let (_, full) = scene.diff(&data2);
        assert!(full);
    }

    #[test]
    fn invalidate_forces_full_upload() {
        let mut scene = RetainedScene::new(4);
        let data = vec![1.0, 2.0, 3.0, 4.0];
        scene.diff(&data);

        scene.invalidate();
        let (_, full) = scene.diff(&data);
        assert!(full);
    }

    #[test]
    fn prev_instance_count() {
        let mut scene = RetainedScene::new(4);
        assert_eq!(scene.prev_instance_count(), 0);

        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        scene.diff(&data);
        assert_eq!(scene.prev_instance_count(), 2);
    }
}
