//! Sharing pixel work across the machine's cores, for the brushes: a big
//! dab touches millions of pixels, each worked out on its own, which is
//! work that divides evenly. Small jobs stay on the calling thread, where
//! they're quicker than starting threads.

/// Pixels a piece of work must cover before it's worth sharing out.
pub(crate) const PARALLEL_PIXELS: usize = 512 * 512;

/// Runs `work` on each job, across the cores if `share`, or on this thread.
pub(crate) fn in_parallel<T: Send>(share: bool, mut jobs: Vec<T>, work: impl Fn(T) + Sync) {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(jobs.len());
    if !share || threads < 2 {
        jobs.into_iter().for_each(work);
        return;
    }
    let per_thread = jobs.len().div_ceil(threads);
    let work = &work;
    std::thread::scope(|scope| {
        while !jobs.is_empty() {
            let share: Vec<T> = jobs.drain(..per_thread.min(jobs.len())).collect();
            scope.spawn(move || share.into_iter().for_each(work));
        }
    });
}

/// Runs `work` over `data` in bands of whole rows, `row` items long,
/// across the cores when there's enough of it: `work` gets each band and
/// the index of its first row.
pub(crate) fn in_bands<T: Send>(data: &mut [T], row: usize, work: impl Fn(usize, &mut [T]) + Sync) {
    let rows = data.len() / row.max(1);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(rows);
    if data.len() < PARALLEL_PIXELS || threads < 2 {
        work(0, data);
        return;
    }
    let band = rows.div_ceil(threads);
    let jobs: Vec<(usize, &mut [T])> =
        data.chunks_mut(band * row).enumerate().map(|(i, chunk)| (i * band, chunk)).collect();
    in_parallel(true, jobs, |(first, chunk)| work(first, chunk));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_cover_every_row_once_with_the_right_numbers() {
        // Big enough to be shared out, and an awkward number of rows.
        let (row, rows) = (700, 1003);
        let mut data = vec![0usize; row * rows];
        in_bands(&mut data, row, |first, band| {
            for (i, v) in band.iter_mut().enumerate() {
                *v += first + i / row + 1;
            }
        });
        assert!(data.chunks(row).enumerate().all(|(y, r)| r.iter().all(|&v| v == y + 1)));
    }
}
