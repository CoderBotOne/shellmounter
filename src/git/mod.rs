use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct GitStatus {
    pub branch: String,
    pub ahead: usize,
    pub behind: usize,
    pub files: Vec<FileStatus>,
    pub clean: bool,
}

#[derive(Debug, Clone)]
pub struct FileStatus {
    pub path: String,
    pub status: FileChange,
    pub staged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChange {
    New, Modified, Deleted, Renamed, TypeChange, Conflicted,
}

impl FileChange {
    pub fn label(&self) -> &'static str {
        match self { FileChange::New=>"A", FileChange::Modified=>"M", FileChange::Deleted=>"D", FileChange::Renamed=>"R", FileChange::TypeChange=>"T", FileChange::Conflicted=>"!!" }
    }
    pub fn color(&self) -> u32 {
        match self { FileChange::New|FileChange::Modified=>0x22c55e, FileChange::Deleted=>0xef4444, FileChange::Renamed|FileChange::TypeChange=>0xf59e0b, FileChange::Conflicted=>0xef4444 }
    }
}

#[derive(Debug, Clone)]
pub struct GitCommit {
    pub oid: String,
    pub short_oid: String,
    pub author: String,
    pub message: String,
    pub time: i64,
    pub branches: Vec<String>,
}

fn run_git(path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(path).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("git {}: {}", args.join(" "), stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn find_repo(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() { return Some(current); }
        if !current.pop() { break; }
    }
    None
}

pub fn get_status(path: &Path) -> Result<GitStatus> {
    let branch = run_git(path, &["branch", "--show-current"]).unwrap_or_else(|_| "detached".into()).trim().to_string();
    let branch = if branch.is_empty() { "HEAD (detached)".into() } else { branch };

    let (ahead, behind) = run_git(path, &["rev-list", "--count", "--left-right", "@{u}...HEAD"])
        .ok().and_then(|s| {
            let parts: Vec<&str> = s.trim().split('\t').collect();
            if parts.len() == 2 { Some((parts[1].parse().unwrap_or(0), parts[0].parse().unwrap_or(0))) }
            else { None }
        }).unwrap_or((0, 0));

    let output = run_git(path, &["status", "--porcelain", "-u"])?;
    let mut files = Vec::new();
    for line in output.lines() {
        if line.len() < 3 { continue; }
        let xy = &line[..2];
        let path = line[3..].trim().to_string();

        // Index (staged)
        let idx = xy.chars().next().unwrap_or(' ');
        if idx != ' ' {
            files.push(FileStatus { path: path.clone(), status: char_to_change(idx), staged: true });
        }
        // Worktree (unstaged)
        let wt = xy.chars().nth(1).unwrap_or(' ');
        if wt != ' ' {
            files.push(FileStatus { path: path.clone(), status: char_to_change(wt), staged: false });
        }
    }

    Ok(GitStatus { branch, ahead, behind, files: files.clone(), clean: files.is_empty() })
}

fn char_to_change(c: char) -> FileChange {
    match c { 'A'|'?' => FileChange::New, 'M' => FileChange::Modified, 'D' => FileChange::Deleted, 'R' => FileChange::Renamed, 'T' => FileChange::TypeChange, 'U' => FileChange::Conflicted, _ => FileChange::Modified }
}

pub fn stage_file(path: &Path, file: &str) -> Result<()> { run_git(path, &["add", file])?; Ok(()) }
pub fn unstage_file(path: &Path, file: &str) -> Result<()> { run_git(path, &["reset", "HEAD", file])?; Ok(()) }
pub fn stage_all(path: &Path) -> Result<()> { run_git(path, &["add", "-A"])?; Ok(()) }
pub fn unstage_all(path: &Path) -> Result<()> { run_git(path, &["reset", "HEAD"])?; Ok(()) }
pub fn commit(path: &Path, message: &str) -> Result<String> { run_git(path, &["commit", "-m", message])?; run_git(path, &["rev-parse", "HEAD"]).map(|s| s.trim().to_string()) }
pub fn push(path: &Path) -> Result<()> { run_git(path, &["push"])?; Ok(()) }

pub fn log(path: &Path, count: usize) -> Result<Vec<GitCommit>> {
    let fmt = "--format=%H|%h|%an|%s|%at";
    let limit = count.to_string();
    let output = run_git(path, &["log", fmt, "-n", &limit, "--all"])?;
    let mut commits = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(5, '|').collect();
        if parts.len() < 5 { continue; }
        commits.push(GitCommit {
            oid: parts[0].to_string(), short_oid: parts[1].to_string(),
            author: parts[2].to_string(), message: parts[3].to_string(),
            time: parts[4].parse().unwrap_or(0), branches: Vec::new(),
        });
    }
    Ok(commits)
}

pub fn branches(path: &Path) -> Result<Vec<String>> {
    let output = run_git(path, &["branch"])?;
    Ok(output.lines().map(|l| l.trim().trim_start_matches("* ").to_string()).collect())
}

pub fn current_branch(path: &Path) -> String {
    run_git(path, &["branch", "--show-current"]).unwrap_or_else(|_| "unknown".into()).trim().to_string()
}

pub fn diff_file_unstaged(path: &Path, file: &str) -> Result<String> { run_git(path, &["diff", file]) }
pub fn diff_file_staged(path: &Path, file: &str) -> Result<String> { run_git(path, &["diff", "--cached", file]) }
