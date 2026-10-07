use std::collections::VecDeque;

pub(super) const MAX_PENDING_OFFLINE_JOBS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct OfflineJobId(u64);

impl OfflineJobId {
    pub(super) const fn from_value(value: u64) -> Self {
        Self(value)
    }

    pub(super) const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum EnqueueError {
    Full,
    IdExhausted,
}

#[derive(Debug)]
pub(super) struct QueuedOfflineJob<T> {
    pub(super) id: OfflineJobId,
    pub(super) job: T,
}

#[derive(Debug)]
pub(super) struct OfflineJobQueue<T> {
    pending: VecDeque<QueuedOfflineJob<T>>,
    next_id: u64,
}

impl<T> Default for OfflineJobQueue<T> {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
            next_id: 1,
        }
    }
}

impl<T> OfflineJobQueue<T> {
    pub(super) fn enqueue(&mut self, job: T) -> Result<OfflineJobId, EnqueueError> {
        if self.pending.len() >= MAX_PENDING_OFFLINE_JOBS {
            return Err(EnqueueError::Full);
        }
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(EnqueueError::IdExhausted)?;
        self.pending.push_back(QueuedOfflineJob {
            id: OfflineJobId(id),
            job,
        });
        Ok(OfflineJobId(id))
    }

    pub(super) fn pop_front(&mut self) -> Option<QueuedOfflineJob<T>> {
        self.pending.pop_front()
    }

    pub(super) fn remove(&mut self, id: OfflineJobId) -> Option<T> {
        let index = self.pending.iter().position(|job| job.id == id)?;
        self.pending.remove(index).map(|job| job.job)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (OfflineJobId, &T)> {
        self.pending.iter().map(|job| (job.id, &job.job))
    }

    pub(super) fn len(&self) -> usize {
        self.pending.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(super) fn remaining_capacity(&self) -> usize {
        MAX_PENDING_OFFLINE_JOBS - self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_start_in_fifo_order_and_removed_jobs_are_skipped() {
        let mut queue = OfflineJobQueue::default();
        let first = queue.enqueue("render A").unwrap();
        let removed = queue.enqueue("freeze B").unwrap();
        let last = queue.enqueue("render C").unwrap();

        assert_eq!(queue.remove(removed), Some("freeze B"));
        assert_eq!(queue.pop_front().unwrap().job, "render A");
        assert_eq!(queue.pop_front().unwrap().job, "render C");
        assert!(queue.is_empty());
        assert!(first.value() < removed.value() && removed.value() < last.value());
    }

    #[test]
    fn queue_rejects_jobs_after_its_bounded_capacity_is_reached() {
        let mut queue = OfflineJobQueue::default();
        for index in 0..MAX_PENDING_OFFLINE_JOBS {
            queue.enqueue(index).unwrap();
        }

        assert_eq!(queue.len(), MAX_PENDING_OFFLINE_JOBS);
        assert_eq!(queue.remaining_capacity(), 0);
        assert_eq!(queue.enqueue(5), Err(EnqueueError::Full));
        queue.pop_front();
        assert_eq!(queue.enqueue(5), Ok(OfflineJobId(5)));
    }
}
