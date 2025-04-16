//! Process management syscalls
use crate::{config::PAGE_SIZE, mm::{copy_to_virt_addr, get_frame_space, MapPermission, PageTable, VPNRange, VirtAddr}, task::{change_program_brk, current_user_token, exit_current_and_run_next, get_syscall_cnt, map_extra, suspend_current_and_run_next, unmap_extra}, timer::get_time_us};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
/// 这里参考了 https://rcore-os.cn/rCore-Tutorial-Book-v3/chapter4/6multitasking-based-on-as.html 下方网友的评论
/// hongjil commented on 2022年11月30日
/// 在本章练习中，我们需要重写sys_get_time()函数，但是不像sys_write() 我们可以方便地用多个切片的方式去重复执行；我能想到的一种比较好的方式是先用一个本地变量去存储TimeVal，然后再拷贝到对应的切片上。
/// 受其启发，我这里也建立一个局部的结构体，然后专门做了个函数用来拷贝过去，以应对分页的问题
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    let res = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000
    };
    copy_to_virt_addr(
        current_user_token(), 
        ts as usize as *const u8, 
        &res    
    );
    0
}

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    if trace_request == 2 {
        return get_syscall_cnt(id) as isize;
    }
    let token = current_user_token();
    let page_table = PageTable::from_token(token);
    let va = VirtAddr::from(id);
    let vpn = va.floor();
    let pte = page_table.translate(vpn);
    if pte.is_none() {
        return -1;
    }
    let pte = pte.unwrap();
    if !pte.user_accessible() {
        return -1;
    }
    if trace_request == 0 {
        if !pte.readable() {
            return -1;
        }
        pte.ppn().get_bytes_array()[va.page_offset()] as isize
    } else if trace_request == 1 {
        if !pte.writable() {
            return -1;
        }
        pte.ppn().get_bytes_array()[va.page_offset()] = data as u8;
        0
    } else {
        -1
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    if 
    start % PAGE_SIZE != 0
    || prot & (!0x7) != 0
    || prot & (0x7) == 0
    {
        return -1;
    }
    let start_va = VirtAddr::from(start);
    let end_va = VirtAddr::from(start + len);
    let start_vpn = start_va.floor();
    let end_vpn = end_va.ceil();
    debug!("[kernel] sys_mmap [{:?},{:?})", start_vpn, end_vpn);
    if get_frame_space() < end_vpn.0 - start_vpn.0 {
        error!("[kernel] kernel space ran out!");
        return -1;
    }
    let vpn_range = VPNRange::new(start_vpn, end_vpn);
    let token = current_user_token();
    let page_table = PageTable::from_token(token);
    debug!("[kernel] from root {:X}", page_table.token());
    for vpn in vpn_range.clone() {
        let pte = page_table.translate(vpn);
        // 页表最后的节点不存在也不会返回
        if pte.is_some() && pte.unwrap().is_valid() {
            error!("[kernel] {:?} has already been mapped to {:?} bits={:#b}!", vpn, pte.unwrap().ppn(), pte.unwrap().bits);
            return -1;
        }
    }
    let mut flags = MapPermission::U;
    if prot & 0x1 != 0 {
        flags |= MapPermission::R;
    }
    if prot & 0x2 != 0 {
        flags |= MapPermission::W;
    }
    if prot & 0x4 != 0 {
        flags |= MapPermission::X;
    }
    map_extra(vpn_range, flags);
    0
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    if start % PAGE_SIZE != 0 {
        return -1;
    }
    let start_va = VirtAddr::from(start);
    let end_va = VirtAddr::from(start + len);
    let start_vpn = start_va.floor();
    let end_vpn = end_va.ceil();
    let vpn_range = VPNRange::new(start_vpn, end_vpn);
    if unmap_extra(vpn_range) {
        0
    } else {
        -1
    }
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
