// Phase 3: KVM/libvirt virtual machine management
// Requires: virsh subprocess or virt crate
// This module will expose:
//   - list_vms() -> Vec<VmInfo>
//   - vm_action(name, action) -> Result<()>
//   - get_vm(name) -> Option<VmInfo>

pub fn is_libvirt_available() -> bool {
    std::path::Path::new("/var/run/libvirt/libvirt-sock").exists()
        || std::process::Command::new("which")
            .arg("virsh")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}
