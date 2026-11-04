//! Slint 回调与任务通道的装配；所有持久写入仍由唯一 DB worker 执行。

mod autosave;
mod connect;
mod datetime;
mod policy;
mod replies;
mod requests;
mod transitions;
mod view;

#[cfg(test)]
mod contract;
