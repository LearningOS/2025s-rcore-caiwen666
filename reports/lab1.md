## 荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 **以下各位** 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

   > 无

2. 此外，我也参考了 **以下资料** ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

   > 微信讨论群中群友关于本章实验 BASE=0 时可以正常运行但设置 BASE=2 却出现问题的讨论

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。

## 实现功能

在内核中添加了一个 id 为 410 的系统调用 sys_trace。该调用可以根据参数的不同进行不同的功能：读写内存中某个地址的数据、统计当前任务调用指定系统调用的次数

## 问答题

### Q1

> 正确进入 U 态后，程序的特征还应有：使用 S 态特权指令，访问 S 态寄存器后会报错。 请同学们可以自行测试这些内容（运行 [三个 bad 测例 (ch2b_bad_*.rs)](https://github.com/LearningOS/rCore-Tutorial-Test-2025S/tree/master/src/bin) ）， 描述程序出错行为，同时注意注明你使用的 sbi 及其版本。

SBI：rust-sbi

SBI版本：RustSBI-QEMU Version 0.2.0-alpha.2

**ch2b_bad_address.rs**

程序试图在 0x0 这个地址写数据，触发 PageFault 的 trap，内核的日志：

```text
[kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x804003a4, kernel killed it.
```

**ch2b_bad_instructions.rs**

程序试图执行 `sret` 指令，由于该指令只能在 S 模式下使用，而程序目前处于 U 模式，于是触发 IllegalInstruction 的 trap，并被内核 kill 掉

**ch2b_bad_register.rs**

程序试图向 `sstatus` 这个 CSR 中写入数据，而 U 模式下不能写 `sstatus`，于是触发 IllegalInstruction 的 trap，并被内核 kill 掉

### Q2

> 深入理解 [trap.S](https://github.com/LearningOS/rCore-Tutorial-Code-2025S/blob/ch3/os/src/trap/trap.S) 中两个函数 `__alltraps` 和 `__restore` 的作用，并回答如下问题:

#### Q2.1

> L40：刚进入 `__restore` 时，`sp` 代表了什么值。请指出 `__restore` 的两种使用情景。

sp 表示当前任务的内核栈栈顶的地址，此时内核栈栈顶存放的数据为 trap 上下文

在 trap 发生时，CPU 跳到 `__alltraps`，`__alltraps` 执行完毕后就继续执行 `__restore`

内核在初始化完毕后会调用 `__restore` 来进行首次进入用户态

#### Q2.2

> L43-L48：这几行汇编代码特殊处理了哪些寄存器？这些寄存器的的值对于进入用户态有何意义？请分别解释。
>
> ```assembly
> ld t0, 32*8(sp)
> ld t1, 33*8(sp)
> ld t2, 2*8(sp)
> csrw sstatus, t0
> csrw sepc, t1
> csrw sscratch, t2
> ```

特殊处理了 `sstatus` 、`sepc`、`sscratch` 寄存器

`sstatus` 记录了发生 trap 之前 CPU 处于哪个特权级

`sepc` 记录了进入用户态之后要执行的指令的地址

`sscratch` 用来辅助用户栈和内核栈的栈指针的切换。在这里把用户栈指针写入 `sscratch` 中，为后面将 `sp` 切换到用户栈做准备

#### Q2.3

> L50-L56：为何跳过了 `x2` 和 `x4`？
>
> ```assembly
> ld x1, 1*8(sp)
> ld x3, 3*8(sp)
> .set n, 5
> .rept 27
>    LOAD_GP %n
>    .set n, n+1
> .endr
> ```

`x2` 寄存器即为 `sp` 寄存器，由于我们在后面恢复上下文的过程中还要从内核栈读数据，所以先不能修改 `sp` 寄存器，而是要到最后面才修改

`x4` 寄存器一般情况下不会使用到，所以无需保存

#### Q2.4

> L60：该指令之后，`sp` 和 `sscratch` 中的值分别有什么意义？
>
> ```assembly
> csrrw sp, sscratch, sp
> ```

sp 的值变为了用户栈的栈指针，sscratch 的值变为了内核栈的栈指针

#### Q2.5

> `__restore`：中发生状态切换在哪一条指令？为何该指令执行之后会进入用户态？

在 `sret` 这条指令

该指令调用后将会设置 CPU 的模式为用户模式，并且跳到 `sepc` 寄存器表示的地址处。在 trap_handler 中已经把 trap 上下文中的 `sepc` 设置为了 `ecall` 指令的下一条指令的地址，并且在 `__restore` 中已经恢复了 trap 上下文。因此调用 `sret` 后会回到用户态

#### Q2.6

> L13：该指令之后，`sp` 和 `sscratch` 中的值分别有什么意义？
>
> ```assembly
> csrrw sp, sscratch, sp
> ```

sp 的值变为内核栈栈顶的地址，sscratch 变为用户栈栈顶的地址

#### Q2.7

> 从 U 态进入 S 态是哪一条指令发生的？

`ecall` 指令