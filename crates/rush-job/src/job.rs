use nix::sys::wait::WaitStatus;
use nix::unistd::Pid;
use std::collections::HashMap;

pub type JobId = u32;

/// State of a job
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobState {
    /// Job is currently running
    Running,
    /// Job has been stopped (Ctrl-Z)
    Stopped,
    /// Job has completed
    Done(i32), // exit code
}

/// Represents a job (pipeline or command)
#[derive(Debug)]
pub struct Job {
    /// Unique job ID
    pub id: JobId,
    /// Process group ID
    pub pgid: Pid,
    /// Command string for display
    pub command: String,
    /// Process IDs in this job (for pipelines)
    pub pids: Vec<Pid>,
    /// Current state
    pub state: JobState,
    /// Is this a foreground job?
    pub foreground: bool,
}

impl Job {
    /// Create a new job
    pub fn new(
        id: JobId,
        pgid: Pid,
        command: String,
        pids: Vec<Pid>,
        foreground: bool,
    ) -> Self {
        Self {
            id,
            pgid,
            command,
            pids,
            state: JobState::Running,
            foreground,
        }
    }

    /// Check if all processes in the job have completed
    pub fn is_completed(&self) -> bool {
        matches!(self.state, JobState::Done(_))
    }

    /// Check if the job is stopped
    pub fn is_stopped(&self) -> bool {
        matches!(self.state, JobState::Stopped)
    }

    /// Check if the job is running
    pub fn is_running(&self) -> bool {
        matches!(self.state, JobState::Running)
    }

    /// Get a status string for display
    pub fn status_string(&self) -> &str {
        match &self.state {
            JobState::Running => "Running",
            JobState::Stopped => "Stopped",
            JobState::Done(_) => "Done",
        }
    }
}

/// Manages all jobs
#[derive(Debug)]
pub struct JobList {
    /// Map of job ID to Job
    jobs: HashMap<JobId, Job>,
    /// Next job ID to assign
    next_job_id: JobId,
    /// Shell's process group ID
    shell_pgid: Pid,
    /// Current foreground job (if any)
    current_job: Option<JobId>,
}

impl JobList {
    /// Create a new job list
    pub fn new(shell_pgid: Pid) -> Self {
        Self {
            jobs: HashMap::new(),
            next_job_id: 1,
            shell_pgid,
            current_job: None,
        }
    }

    /// Add a new job
    pub fn add_job(
        &mut self,
        pgid: Pid,
        command: String,
        pids: Vec<Pid>,
        foreground: bool,
    ) -> JobId {
        let id = self.next_job_id;
        self.next_job_id += 1;

        let job = Job::new(id, pgid, command, pids, foreground);
        self.jobs.insert(id, job);

        if foreground {
            self.current_job = Some(id);
        }

        id
    }

    /// Get a job by ID
    pub fn get_job(&self, id: JobId) -> Option<&Job> {
        self.jobs.get(&id)
    }

    /// Get a mutable reference to a job by ID
    pub fn get_job_mut(&mut self, id: JobId) -> Option<&mut Job> {
        self.jobs.get_mut(&id)
    }

    /// Remove a job
    pub fn remove_job(&mut self, id: JobId) -> Option<Job> {
        if self.current_job == Some(id) {
            self.current_job = None;
        }
        self.jobs.remove(&id)
    }

    /// Get the current (most recent) job
    pub fn current_job(&self) -> Option<&Job> {
        self.current_job.and_then(|id| self.get_job(id))
    }

    /// Get all jobs
    pub fn jobs(&self) -> impl Iterator<Item = &Job> {
        self.jobs.values()
    }

    /// Get all jobs as a sorted list
    pub fn jobs_sorted(&self) -> Vec<&Job> {
        let mut jobs: Vec<_> = self.jobs.values().collect();
        jobs.sort_by_key(|job| job.id);
        jobs
    }

    /// Update job state based on wait status
    pub fn update_job_status(&mut self, pid: Pid, status: WaitStatus) -> Option<JobId> {
        // Find the job containing this PID
        let job_id = self.jobs.iter().find_map(|(id, job)| {
            if job.pids.contains(&pid) {
                Some(*id)
            } else {
                None
            }
        })?;

        if let Some(job) = self.get_job_mut(job_id) {
            match status {
                WaitStatus::Exited(_, code) => {
                    job.state = JobState::Done(code);
                }
                WaitStatus::Signaled(_, signal, _) => {
                    // Terminated by signal, use 128 + signal number as exit code
                    job.state = JobState::Done(128 + signal as i32);
                }
                WaitStatus::Stopped(_, _) => {
                    job.state = JobState::Stopped;
                }
                WaitStatus::Continued(_) => {
                    job.state = JobState::Running;
                }
                _ => {}
            }
        }

        Some(job_id)
    }

    /// Remove all completed jobs
    pub fn clean_completed(&mut self) -> Vec<Job> {
        let completed: Vec<_> = self
            .jobs
            .iter()
            .filter(|(_, job)| job.is_completed())
            .map(|(id, _)| *id)
            .collect();

        completed
            .into_iter()
            .filter_map(|id| self.remove_job(id))
            .collect()
    }

    /// Get shell's process group ID
    pub fn shell_pgid(&self) -> Pid {
        self.shell_pgid
    }

    /// Get the current job ID
    pub fn current_job_id(&self) -> Option<JobId> {
        self.current_job
    }

    /// Get the previous job ID (second most recent)
    pub fn previous_job(&self) -> Option<JobId> {
        // Return the second highest job ID that isn't the current one
        let mut job_ids: Vec<_> = self.jobs.keys().copied().collect();
        job_ids.sort_by(|a, b| b.cmp(a)); // Descending order

        job_ids.into_iter().find(|&id| Some(id) != self.current_job)
    }

    /// Disown a specific job (remove from job control without killing it)
    pub fn disown_job(&mut self, id: JobId) {
        self.remove_job(id);
    }

    /// Disown all jobs, optionally only running jobs
    pub fn disown_all(&mut self, running_only: bool) {
        let to_disown: Vec<_> = self
            .jobs
            .iter()
            .filter(|(_, job)| !running_only || job.is_running())
            .map(|(id, _)| *id)
            .collect();

        for id in to_disown {
            self.remove_job(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_creation() {
        let shell_pgid = Pid::from_raw(1000);
        let mut job_list = JobList::new(shell_pgid);

        let pgid = Pid::from_raw(2000);
        let pids = vec![Pid::from_raw(2000), Pid::from_raw(2001)];
        let id = job_list.add_job(pgid, "ls | grep test".to_string(), pids, false);

        assert_eq!(id, 1);
        assert_eq!(job_list.jobs().count(), 1);

        let job = job_list.get_job(id).unwrap();
        assert_eq!(job.id, 1);
        assert_eq!(job.pgid, pgid);
        assert!(job.is_running());
    }

    #[test]
    fn test_job_state_transitions() {
        let shell_pgid = Pid::from_raw(1000);
        let mut job_list = JobList::new(shell_pgid);

        let pgid = Pid::from_raw(2000);
        let pid = Pid::from_raw(2000);
        let id = job_list.add_job(pgid, "sleep 10".to_string(), vec![pid], false);

        // Initially running
        assert!(job_list.get_job(id).unwrap().is_running());

        // Update to stopped
        job_list.update_job_status(pid, WaitStatus::Stopped(pid, nix::sys::signal::SIGTSTP));
        assert!(job_list.get_job(id).unwrap().is_stopped());

        // Update to continued
        job_list.update_job_status(pid, WaitStatus::Continued(pid));
        assert!(job_list.get_job(id).unwrap().is_running());

        // Update to done
        job_list.update_job_status(pid, WaitStatus::Exited(pid, 0));
        assert!(job_list.get_job(id).unwrap().is_completed());
    }
}
