//! Types related to task management

use super::TaskContext;

/// The task control block (TCB) of a task.
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// 记录系统调用次数
    /// 这里参考了微信讨论群1群中昵称为“恒”的网友提出的“有没有大佬遇到第三章 make run base=0 可以 PASS，但是BASE=2 get_time拿的时间总是0的情况”问题以及陈宏毅助教的解答“这个现在初步判断是初始化的时候把内核初始栈爆了”
    /// 原来这里数组的长度设为 512，然后 BASE=2 评测时卡死，于是把数据改小了一点发现能跑通了
    /// 不过仍然存在的疑问是这里作为全局变量的一部分理论上应该是在.bss段留好空间的。助教说的爆栈有点奇怪
    pub task_syscall_trace: [usize; 411]
}

/// The status of a task
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Exited,
}
