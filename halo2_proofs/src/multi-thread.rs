use std::thread;

extern crate num_cpus;

fn multi_thread_transform() {
    // 获取当前CPU的内核数（逻辑），执行多线程并行策略
    let cpu_num = num_cpus::get();

    // 创建CPU核心数哥线程
    for _i in 0..cpu_num {

    }

    thread::spawn(|| {
        println!("multi_thread!");
    }
    );

}