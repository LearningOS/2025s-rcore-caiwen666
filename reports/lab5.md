## 荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 **以下各位** 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

   > 微信讨论群的群友

2. 此外，我也参考了 **以下资料** ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

   > 无

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。

## 实现功能

实现了指导书中所要求的死锁检测的功能

## 实验所用时间

约5个小时

## 问答题

### Q1

> 在我们的多线程实现中，当主线程 (即 0 号线程) 退出时，视为整个进程退出， 此时需要结束该进程管理的所有线程并回收其资源。 - 需要回收的资源有哪些？ - 其他线程的 TaskControlBlock 可能在哪些位置被引用，分别是否需要回收，为什么？

需要回收内存空间（当前线程所处的内核栈不回收），所有线程的 TCB，文件描述符，mutex，condvar，semaphore ，用于 sleep 的定时器

会在 PCB 中被引用

会在 mutex,condvar,semaphore,定时器这些唤醒队列中被引用

会在 内核的调度队列中被引用

应该都需要回收，因为进程结束了，其包含的线程也没用了。如果一个没回收，就会导致引用计数不能归零，从而导致内存泄漏

### Q2

> 对比以下两种 `Mutex` 中的实现，二者有什么区别？这些区别可能会导致什么问题？
>
> ```rust
>  1impl Mutex for Mutex1 {
>  2    fn lock(&self) {
>  3        loop {
>  4            let mut mutex_inner = self.inner.exclusive_access();
>  5            if mutex_inner.locked {
>  6                mutex_inner.wait_queue.push_back(current_task().unwrap());
>  7                drop(mutex_inner);
>  8                block_current_and_run_next();
>  9            } else {
> 10                mutex_inner.locked = true;
> 11                break;
> 12            }
> 13        }
> 14    }
> 15
> 16    fn unlock(&self) {
> 17        let mut mutex_inner = self.inner.exclusive_access();
> 18        assert!(mutex_inner.locked);
> 19        mutex_inner.locked = false;
> 20        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
> 21            add_task(waking_task);
> 22        }
> 23    }
> 24}
> 25
> 26impl Mutex for Mutex2 {
> 27    fn lock(&self) {
> 28        let mut mutex_inner = self.inner.exclusive_access();
> 29        if mutex_inner.locked {
> 30            mutex_inner.wait_queue.push_back(current_task().unwrap());
> 31            drop(mutex_inner);
> 32            block_current_and_run_next();
> 33        } else {
> 34            mutex_inner.locked = true;
> 35        }
> 36    }
> 37
> 38    fn unlock(&self) {
> 39        let mut mutex_inner = self.inner.exclusive_access();
> 40        assert!(mutex_inner.locked);
> 41        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
> 42            add_task(waking_task);
> 43        } else {
> 44            mutex_inner.locked = false;
> 45        }
> 46    }
> 47}
> ```

Mutex1 是解锁的时候立即更新锁状态并唤醒一个线程。线程被唤醒的时候再次检查一下是否锁是否可以获取，然后再加锁。

Mutex2 是解锁的时候先不着急更新锁的状态，而是唤醒另一个线程。另一个线程被唤醒的时候可以保证锁是加锁状态，从而唤醒后立刻可进入临界区、

我认为 Mutex1 的实现会有公平性的问题。线程在被唤醒和真正被调度中间存在间隔。在这个间隔中，锁是处于释放的状态，在这个间隔中被调度的线程都可以立刻拿锁，这使得最早申请拿锁的线程反而要在后面才能拿到锁