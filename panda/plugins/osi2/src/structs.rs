use std::mem::size_of;
use panda::prelude::*;
use panda::plugins::osi2::{osi_static, OsiType, find_per_cpu_address};
use panda::GuestType;
use crate::symbol_table;

/// Max length of process command (`comm` field in task_struct)
pub const TASK_COMM_LEN: usize = 16;

//#################################################################
//#################### Task related structures ####################
//#################################################################

#[derive(Debug)]
pub struct Version {
    pub a: target_ptr_t,
    pub b: target_ptr_t,
    pub c: target_ptr_t,
}

// Digging around in kernel source for 3.7 traced these fields' types, kuid_t and kgid_t,
// through a few definitions and found they were both structs which hold a single value of type
// git_t or uid_t which are, in that kernel version, just unsigned ints
#[derive(OsiType, Debug)]
#[osi(type_name = "cred")]
pub struct CredStruct {
    pub uid: target_ptr_t, // type unsigned int
    pub gid: target_ptr_t, // type unsigned int
    pub euid: target_ptr_t, // type unsigned int
    pub egid: target_ptr_t, // type unsigned int
}

#[derive(OsiType, Debug)]
#[osi(type_name = "mm_struct")]
pub struct MmStruct {
    pub pgd: u32, // type *unnamed_bunch_of_stuff_3
    pub arg_start: target_ptr_t, // type long unsigned int
    pub start_brk: target_ptr_t, // type long unsigned int
    pub brk: target_ptr_t, // type long unsigned int
    pub start_stack: target_ptr_t, // type long unsigned int
}

#[derive(OsiType, Debug)]
#[osi(type_name = "task_struct")]
pub struct TaskStruct {
    // Only one of tasks or next_task will exist as a field
    pub tasks: target_ptr_t, // type list_head
    //next_task: target_ptr_t, // type ??

    pub pid: u32, // type int
    pub tgid: u32, //type int
    pub group_leader: target_ptr_t, // type *task_struct
    pub thread_group: target_ptr_t, // type list_head

    // Only one of real_parent or p_opptr will exist as a field
    pub real_parent: target_ptr_t, // type *task_struct 
    //p_opptr: target_ptr_t, // type ??

    // Only one of parent or p_pptr will exist as a field
    pub parent: target_ptr_t, // type *task_struct
    //p_pptr: target_ptr_t, // type ??

    pub mm: target_ptr_t, // type *mm_struct
    pub stack: target_ptr_t, // type *void
    pub real_cred: target_ptr_t, // type *cred
    pub cred: target_ptr_t, // type *cred
    pub comm: [u8; TASK_COMM_LEN], // type char[]
    pub files: target_ptr_t, // type *files_struct
    pub start_time: target_ptr_t, // type long long unsigned int

}

#[derive(Debug)]
pub struct CosiProc {
    /*
    pub asid: u32,
    pub start_time: target_ptr_t,
    pub name: String,
    pub pid: u32,
    pub ppid: u32,
    pub taskd: target_ptr_t,
     */
    pub addr: target_ptr_t,
    pub task: TaskStruct,
    pub name: String,
    pub ppid: u32,
    pub mm: MmStruct,
    pub asid: u32,
    pub taskd: target_ptr_t,
}

impl CosiProc {
    pub fn get_current_process(cpu: &mut CPUState) -> Option<CosiProc> {
        let curr_task_addr = find_per_cpu_address(cpu, "current_task").ok()?;
        CosiProc::new(cpu, curr_task_addr)
    }
    pub fn new(cpu: &mut CPUState, addr: target_ptr_t) -> Option<CosiProc> {
        let task = TaskStruct::osi_read(cpu, addr).ok()?;
        let mm_ptr = task.mm;
        let mm = MmStruct::osi_read(cpu, mm_ptr).ok()?;
        let asid: u32 = mm.pgd;

        let comm_data = task.comm;
        let task_comm_len = comm_data
            .iter()
            .position(|&x| x == 0u8)
            .unwrap_or(TASK_COMM_LEN);
        let name = String::from_utf8_lossy(&comm_data[..task_comm_len]).into_owned();
        let parent = TaskStruct::osi_read(cpu, task.parent).unwrap();
        let ppid = parent.pid;
        let taskd = task.group_leader;

        Some( CosiProc {
            addr: addr,
            task: task,
            name: name,
            ppid: ppid,
            mm: mm,
            asid: asid,
            taskd: taskd,
        })
    }
}

#[derive(Debug)]
pub struct CosiThread {
    pub tid: u32,
    pub pid: u32,
    // Maybe in the future want to have more mature thread_struct represenation
    // but old OSI doesn't use it
}

impl CosiThread {
    pub fn get_current_thread(cpu: &mut CPUState) -> Option<CosiThread> {
        let c_proc = CosiProc::get_current_process(cpu)?;
        Some( CosiThread {
            tid: c_proc.task.pid,
            pid: c_proc.task.tgid,
        })
    }
}

//#################################################################
//#################### File related structures ####################
//#################################################################
#[derive(OsiType, Debug)]
#[osi(type_name = "vm_area_struct")]
pub struct VmAreaStruct {
    pub vm_mm: target_ptr_t, // type *mm_struct
    pub vm_start: target_ptr_t, // type long unsigned int
    pub vm_end: target_ptr_t, // type long unsigned int
    pub vm_next: target_ptr_t, // type *vm_area_struct
    pub vm_file: target_ptr_t, // type *file
    pub vm_flags: target_ptr_t, // type long unsigned int
}

#[derive(OsiType, Debug)]
#[osi(type_name = "callback_head")]
pub struct CallbackHead {
    pub func: target_ptr_t, // type *function
    pub next: target_ptr_t, // type *callback_head
}
pub const QSTR_NAME_LEN: usize = 256;

#[derive(OsiType, Debug)]
#[osi(type_name = "qstr")]
pub struct Qstr {
    pub unnamed_field_0: u64, // type union {struct { HASH_LEN_DECLARE; }; u64 hash_len;}
    pub name: target_ptr_t, // type *char
    //name: [u8; QSTR_NAME_LEN] // trying it this way for easier reading?
}

#[derive(OsiType, Debug)]
#[osi(type_name = "dentry")]
pub struct Dentry {
    pub d_parent: target_ptr_t, // type *dentry
    //d_name: target_ptr_t, // type qstr (struct qstr { union { struct {HASH_LEN_DECLARE;}; u64 hash_len; } const unsigned char *name;})
    #[osi(osi_type)]
    pub d_name: Qstr,
}

#[derive(OsiType, Debug)]
#[osi(type_name = "mount")]
pub struct Mount {
    pub mnt_mountpoint: target_ptr_t, // type *dentry
}

#[derive(OsiType, Debug)]
#[osi(type_name = "vfsmount")]
pub struct VfsMount {
    pub mnt_flags: i32, // type int
    pub mnt_root: target_ptr_t, // type *dentry
}

#[derive(OsiType, Debug)]
#[osi(type_name = "path")]
pub struct Path {
    pub dentry: target_ptr_t, // type *dentry
    pub mnt: target_ptr_t, // type *vfsmount
}

#[derive(OsiType, Debug)]
#[osi(type_name = "file")]
pub struct File {
    #[osi(osi_type)]
    pub f_path: Path, // type Path
    pub f_pos: target_ptr_t, // type long long int
}

impl File {
    fn read_dentry_name(&self, cpu: &mut CPUState, is_mnt: bool) -> Option<String> {
        let mut ret = "".to_owned();
        let mut current_dentry_parent = if is_mnt {
                                        // next read name stuff from vfsmount too
                                        let mnt = VfsMount::osi_read(cpu, self.f_path.mnt).ok()?;
                                        let mount_vol = symbol_table().type_from_name("mount").unwrap();
                                        let off = mount_vol.fields["mnt"].offset as u64;
                                        let mount_struct = Mount::osi_read(cpu, self.f_path.mnt - off).ok()?;
                                        mount_struct.mnt_mountpoint
                                    } else {
                                        self.f_path.dentry
                                    };
        let mut current_dentry: target_ptr_t = 0xdead00af;

        while current_dentry_parent != current_dentry {
            current_dentry = current_dentry_parent;
            let dentry_struct = Dentry::osi_read(cpu, current_dentry).ok()?;
            current_dentry_parent = dentry_struct.d_parent;
            let mut name_ptr = dentry_struct.d_name.name;
            let name = cpu.mem_read_string(name_ptr);
    
            let term = if ret == "" || is_mnt {
                &""
            } else {
                "/"
            };

            if &name == "/" || current_dentry == current_dentry_parent {
                ret = name.to_owned() + &ret
            } else {
                ret = name.to_owned() + term + &ret
            }
        }
    
        match ret.as_str() {
            "/" => Some("".to_owned()),
            _ => Some(ret),
        }
    }

    pub fn read_name(&self, cpu: &mut CPUState) -> Option<String> {
        // read file->path->dentry to get a pointer to the first dentry we want to read;
        let d_name = self.read_dentry_name(cpu, false)?;
        let m_name = self.read_dentry_name(cpu, true)?;
        Some(m_name + &d_name)
    }
}

#[derive(OsiType, Debug)]
#[osi(type_name = "fdtable")]
pub struct Fdtable {
    pub close_on_exec: target_ptr_t, // type *long unsigned int
    pub fd: target_ptr_t, // type **file
    pub full_fds_bits: target_ptr_t, // type *long unsigned int
    pub max_fds: u32, // type unsigned int
    pub open_fds: target_ptr_t, // type *long unsigned int | used as a bit vector, if nth bit is set, fd n is open

    // It doesn't seem like we'll need these, but maybe
    //rcu: CallbackHead, // type callbackhead
    pub rcu: target_ptr_t, // placeholder for compilation until I can figure out what to do
}

#[derive(OsiType, Debug)]
#[osi(type_name = "files_struct")]
pub struct FilesStruct {
    pub fd_array: [target_ptr_t; 64], // type *file[] | default length is defined as BITS_IN_LONG, might need to make this smarter/dependant on the system
    pub fdt: target_ptr_t, // type *fdtable
    #[osi(osi_type)]
    pub fdtab: Fdtable,
}

// Cosi struct for holding and accessing information about a file struct
#[derive(Debug)]
pub struct CosiFile {
    pub addr: target_ptr_t,
    pub file_struct: File,
    pub name: String,
    pub fd: u32,
}

impl CosiFile {
    pub fn new(cpu: &mut CPUState, addr: target_ptr_t, fd: u32) -> Option<Self> {
        let file = File::osi_read(cpu, addr).ok()?;
        let name = file.read_name(cpu)?;
        return Some(CosiFile {
        addr: addr,
        file_struct: file,
        name: name,
        fd: fd,
        })
    }
}

#[derive(Debug)]
pub struct CosiFiles {
    pub files: Vec<CosiFile>,
}

impl CosiFiles {
    pub fn get_current_files(cpu: &mut CPUState) -> Option<CosiFiles> {
        let c_proc = CosiProc::get_current_process(cpu)?;
        CosiFiles::new(cpu, c_proc.task.files)
    }
    pub fn new(cpu: &mut CPUState, addr: target_ptr_t) -> Option<Self> {
        let mut file_vec = Vec::<CosiFile>::new();
        let files = FilesStruct::osi_read(cpu, addr).ok()?;
        //let fdtab = files.fdt;
        //let fdtable = Fdtable::osi_read(cpu, fdtab).ok()?;
        let max_fds = files.fdtab.max_fds;
        let open_fds = u32::read_from_guest(cpu, files.fdtab.open_fds).unwrap();
        let mut fd_ptr = files.fdtab.fd;

        let step = size_of::<target_ptr_t>() as u64;
        for idx in 0..max_fds {
            let fd = target_ptr_t::read_from_guest(cpu, fd_ptr).unwrap();
            if fd == 0 {
                break;
            }
            let bv_check = open_fds >> idx;
            if bv_check == 0 {
                break;
            } else if bv_check % 2 == 0{
                fd_ptr += step;
                continue;
            } else {
                fd_ptr += step;
                match CosiFile::new(cpu, fd, idx) {
                    Some(f_info) => file_vec.push(f_info),
                    None => (),
                };
            }
        }
        Some( CosiFiles {
            files: file_vec,
        })
    }
}