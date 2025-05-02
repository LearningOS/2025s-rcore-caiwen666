//! Implementation of  [`ProcessControlBlock`]

use super::id::RecycleAllocator;
use super::manager::insert_into_pid2process;
use super::TaskControlBlock;
use super::{add_task, SignalFlags};
use super::{pid_alloc, PidHandle};
use crate::fs::{File, Stdin, Stdout};
use crate::mm::{translated_refmut, MemorySet, KERNEL_SPACE};
use crate::sync::{Condvar, Mutex, Semaphore, UPSafeCell};
use crate::trap::{trap_handler, TrapContext};
use alloc::collections::btree_map::BTreeMap;
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefMut;

/// Process Control Block
pub struct ProcessControlBlock {
    /// immutable
    pub pid: PidHandle,
    /// mutable
    inner: UPSafeCell<ProcessControlBlockInner>,
}

/**
 * 由于一开始对这个死锁检测算法不太明白，印象里好像群里面有人讨论过，于是就在微信群里搜索聊天记录
 * 一位助教说不要陷入银行家算法，所以我就反复看指导书里的算法过程，不再去管什么银行家算法了
 * 其中 need 这个东西，我一开始不太明白，后来有个群友在群里说他是线程申请资源的时候将对应 need 加一，申请完毕之后将对应 need 减一
 * 然后具体的实现我自己又脑补了一下，最终写出来
 */
/// 死锁检测需要的数据结构
#[derive(Default, Debug)]
pub struct DeadlockDetectData {
    /// 资源可用数量
    pub available: BTreeMap<usize, usize>,
    /// 线程已经分得资源的数量
    pub allocation: BTreeMap<usize, BTreeMap<usize, usize>>,
    /// 线程需要的资源的数量
    pub need: BTreeMap<usize, BTreeMap<usize, usize>>
}

impl DeadlockDetectData {
    /// 检查是否会有死锁
    pub fn check_dead_lock(&self) -> bool {
        // println!("in!!!!!!!!!!!!!!!!!!!");
        // println!("{:#?}", self);
        let mut work = self.available.clone();
        let mut finish = BTreeMap::new();
        for i in self.need.keys() {
            finish.insert(*i, false);
        }
        loop {
            if let Some((tid, is_finish)) = finish
                .iter_mut()
                .find(|(tid, is_finish)| {
                    if **is_finish {
                        return false;
                    }
                    let mut ok = true;
                    for (rid, need_count) in self.need.get(*tid).unwrap() {
                        if *need_count > *(work.get(rid).unwrap()) {
                            ok = false;
                            break;
                        }
                    }
                    ok
                })
            {
                let allocation = self.allocation.get(tid).unwrap();
                for (rid, work_count) in &mut work {
                    // println!("{:#?}\n{:#?}", rid, allocation);
                    *work_count += allocation.get(rid).unwrap();
                    *is_finish = true;
                }
            } else {
                break;
            }
        }
        if finish.iter().find(|(_, is_finish)| !(**is_finish)).is_some() {
            // println!("failed!");
            false
        } else {
            // println!("okok");
            true
        }
    }
    /// 线程创建
    pub fn create_thread(&mut self, tid: usize) {
        let mut empty_map = BTreeMap::new();
        for rid in self.available.keys() {
            empty_map.insert(*rid, 0);
        }
        self.allocation.insert(tid, empty_map.clone());
        self.need.insert(tid, empty_map);
    }
    /// 线程释放
    pub fn release_thread(&mut self, tid: usize) {
        self.allocation.remove(&tid);
        self.need.remove(&tid);
    }
    /// 插入资源
    pub fn create_res(&mut self, rid: usize, count: usize) {
        self.available.insert(rid, count);
        for i in self.allocation.values_mut() {
            i.insert(rid, 0);
        }
        for i in self.need.values_mut() {
            i.insert(rid, 0);
        }
    }
    /// 获取资源
    pub fn down(&mut self, rid: usize, tid: usize) {
        let allocation = self.allocation.get_mut(&tid).unwrap();
        let pre = allocation.get_mut(&rid).unwrap();
        *pre += 1;
        let available = &mut self.available;
        let pre = available.get_mut(&rid).unwrap();
        *pre -= 1;
    }
    /// 增加需求
    pub fn need(&mut self, tid: usize, rid: usize) {
        let need_map = self.need.get_mut(&tid).unwrap();
        let pre = need_map.get_mut(&rid).unwrap();
        *pre += 1;
    }
    /// 增加需求
    pub fn satisfy(&mut self, tid: usize, rid: usize) {
        let need_map = self.need.get_mut(&tid).unwrap();
        let pre = need_map.get_mut(&rid).unwrap();
        *pre -= 1;
    }
    /// 释放资源
    pub fn up(&mut self, rid: usize, tid: usize) {
        let allocation = self.allocation.get_mut(&tid).unwrap();
        let pre = allocation.get_mut(&rid).unwrap();
        *pre -= 1;
        let available = &mut self.available;
        let pre = available.get_mut(&rid).unwrap();
        *pre += 1;
    }
}

/// Inner of Process Control Block
pub struct ProcessControlBlockInner {
    /// is zombie?
    pub is_zombie: bool,
    /// memory set(address space)
    pub memory_set: MemorySet,
    /// parent process
    pub parent: Option<Weak<ProcessControlBlock>>,
    /// children process
    pub children: Vec<Arc<ProcessControlBlock>>,
    /// exit code
    pub exit_code: i32,
    /// file descriptor table
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,
    /// signal flags
    pub signals: SignalFlags,
    /// tasks(also known as threads)
    pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
    /// task resource allocator
    pub task_res_allocator: RecycleAllocator,
    /// mutex list
    pub mutex_list: Vec<Option<Arc<dyn Mutex>>>,
    /// semaphore list
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    /// condvar list
    pub condvar_list: Vec<Option<Arc<Condvar>>>,
    /// 是否开启死锁检测
    pub deadlock_detect: bool,
    /// mutex 的死锁检测
    pub mutex_deadlock_detect: DeadlockDetectData,
    /// semaphore 的死锁检测
    pub semaphore_deadlock_detect: DeadlockDetectData
}

impl ProcessControlBlockInner {
    #[allow(unused)]
    /// get the address of app's page table
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    /// allocate a new file descriptor
    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            fd
        } else {
            self.fd_table.push(None);
            self.fd_table.len() - 1
        }
    }
    /// allocate a new task id
    pub fn alloc_tid(&mut self) -> usize {
        self.task_res_allocator.alloc()
    }
    /// deallocate a task id
    pub fn dealloc_tid(&mut self, tid: usize) {
        self.task_res_allocator.dealloc(tid)
    }
    /// the count of tasks(threads) in this process
    pub fn thread_count(&self) -> usize {
        self.tasks.len()
    }
    /// get a task with tid in this process
    pub fn get_task(&self, tid: usize) -> Arc<TaskControlBlock> {
        self.tasks[tid].as_ref().unwrap().clone()
    }
}

impl ProcessControlBlock {
    /// inner_exclusive_access
    pub fn inner_exclusive_access(&self) -> RefMut<'_, ProcessControlBlockInner> {
        self.inner.exclusive_access()
    }
    /// new process from elf file
    pub fn new(elf_data: &[u8]) -> Arc<Self> {
        trace!("kernel: ProcessControlBlock::new");
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        // allocate a pid
        let pid_handle = pid_alloc();
        let process = Arc::new(Self {
            pid: pid_handle,
            inner: unsafe {
                UPSafeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> stderr
                        Some(Arc::new(Stdout)),
                    ],
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),
                    deadlock_detect: false,
                    mutex_deadlock_detect: DeadlockDetectData::default(),
                    semaphore_deadlock_detect: DeadlockDetectData::default()
                })
            },
        });
        // create a main thread, we should allocate ustack and trap_cx here
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&process),
            ustack_base,
            true,
        ));
        // prepare trap_cx of main thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        let ustack_top = task_inner.res.as_ref().unwrap().ustack_top();
        let kstack_top = task.kstack.get_top();
        drop(task_inner);
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            ustack_top,
            KERNEL_SPACE.exclusive_access().token(),
            kstack_top,
            trap_handler as usize,
        );
        // add main thread to the process
        let mut process_inner = process.inner_exclusive_access();
        process_inner.tasks.push(Some(Arc::clone(&task)));
        drop(process_inner);
        insert_into_pid2process(process.getpid(), Arc::clone(&process));
        // add main thread to scheduler
        add_task(task);
        process
    }

    /// Only support processes with a single thread.
    pub fn exec(self: &Arc<Self>, elf_data: &[u8], args: Vec<String>) {
        trace!("kernel: exec");
        assert_eq!(self.inner_exclusive_access().thread_count(), 1);
        // memory_set with elf program headers/trampoline/trap context/user stack
        trace!("kernel: exec .. MemorySet::from_elf");
        let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        let new_token = memory_set.token();
        // substitute memory_set
        trace!("kernel: exec .. substitute memory_set");
        self.inner_exclusive_access().memory_set = memory_set;
        // then we alloc user resource for main thread again
        // since memory_set has been changed
        trace!("kernel: exec .. alloc user resource for main thread again");
        let task = self.inner_exclusive_access().get_task(0);
        let mut task_inner = task.inner_exclusive_access();
        task_inner.res.as_mut().unwrap().ustack_base = ustack_base;
        task_inner.res.as_mut().unwrap().alloc_user_res();
        task_inner.trap_cx_ppn = task_inner.res.as_mut().unwrap().trap_cx_ppn();
        // push arguments on user stack
        trace!("kernel: exec .. push arguments on user stack");
        let mut user_sp = task_inner.res.as_mut().unwrap().ustack_top();
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>();
        let argv_base = user_sp;
        let mut argv: Vec<_> = (0..=args.len())
            .map(|arg| {
                translated_refmut(
                    new_token,
                    (argv_base + arg * core::mem::size_of::<usize>()) as *mut usize,
                )
            })
            .collect();
        *argv[args.len()] = 0;
        for i in 0..args.len() {
            user_sp -= args[i].len() + 1;
            *argv[i] = user_sp;
            let mut p = user_sp;
            for c in args[i].as_bytes() {
                *translated_refmut(new_token, p as *mut u8) = *c;
                p += 1;
            }
            *translated_refmut(new_token, p as *mut u8) = 0;
        }
        // make the user_sp aligned to 8B for k210 platform
        user_sp -= user_sp % core::mem::size_of::<usize>();
        // initialize trap_cx
        trace!("kernel: exec .. initialize trap_cx");
        let mut trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            task.kstack.get_top(),
            trap_handler as usize,
        );
        trap_cx.x[10] = args.len();
        trap_cx.x[11] = argv_base;
        *task_inner.get_trap_cx() = trap_cx;
    }

    /// Only support processes with a single thread.
    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        trace!("kernel: fork");
        let mut parent = self.inner_exclusive_access();
        assert_eq!(parent.thread_count(), 1);
        // clone parent's memory_set completely including trampoline/ustacks/trap_cxs
        let memory_set = MemorySet::from_existed_user(&parent.memory_set);
        // alloc a pid
        let pid = pid_alloc();
        // copy fd table
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        // create child process pcb
        let child = Arc::new(Self {
            pid,
            inner: unsafe {
                UPSafeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),
                    deadlock_detect: parent.deadlock_detect,
                    mutex_deadlock_detect: DeadlockDetectData::default(),
                    semaphore_deadlock_detect: DeadlockDetectData::default()
                })
            },
        });
        // add child
        parent.children.push(Arc::clone(&child));
        // create main thread of child process
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&child),
            parent
                .get_task(0)
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .ustack_base(),
            // here we do not allocate trap_cx or ustack again
            // but mention that we allocate a new kstack here
            false,
        ));
        // attach task to child process
        let mut child_inner = child.inner_exclusive_access();
        child_inner.tasks.push(Some(Arc::clone(&task)));
        drop(child_inner);
        // modify kstack_top in trap_cx of this thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        trap_cx.kernel_sp = task.kstack.get_top();
        drop(task_inner);
        insert_into_pid2process(child.getpid(), Arc::clone(&child));
        // add this thread to scheduler
        add_task(task);
        child
    }
    /// get pid
    pub fn getpid(&self) -> usize {
        self.pid.0
    }
}
