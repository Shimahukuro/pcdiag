use std::time::Instant;

use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, CpuCollection, CpuFeatures,
    CpuPackage, CpuTopology, FieldCollectionResult, FieldCollectionStatus, SystemArchitecture,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuCollectionResult {
    pub collection: CpuCollection,
    pub status: CollectorResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CpuSnapshot {
    architecture: Option<SystemArchitecture>,
    topology: CpuTopology,
    packages: Option<Vec<CpuPackage>>,
    features: CpuFeatures,
    messages: Vec<CollectionMessage>,
    missing: Vec<MissingField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MissingField {
    path: String,
    status: FieldCollectionStatus,
    code: &'static str,
    native_code: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CollectionFailure {
    code: &'static str,
    native_code: Option<i64>,
    message: &'static str,
}

pub fn collect_cpu() -> CpuCollectionResult {
    let started = Instant::now();
    build_result(platform::collect(), elapsed_ms(started))
}

fn build_result(
    snapshot: Result<CpuSnapshot, CollectionFailure>,
    duration_ms: u64,
) -> CpuCollectionResult {
    match snapshot {
        Ok(snapshot) => {
            let status = if snapshot.missing.is_empty() {
                CollectorStatus::Success
            } else {
                CollectorStatus::Partial
            };
            CpuCollectionResult {
                collection: CpuCollection {
                    architecture: snapshot.architecture,
                    topology: snapshot.topology,
                    packages: snapshot.packages,
                    features: snapshot.features,
                },
                status: CollectorResult {
                    name: CollectorName::Cpu,
                    status,
                    duration_ms,
                    messages: snapshot.messages,
                    fields: snapshot
                        .missing
                        .into_iter()
                        .map(|missing| FieldCollectionResult {
                            path: missing.path,
                            status: missing.status,
                            code: missing.code.into(),
                            native_code: missing.native_code,
                        })
                        .collect(),
                },
            }
        }
        Err(failure) => CpuCollectionResult {
            collection: null_collection(),
            status: CollectorResult {
                name: CollectorName::Cpu,
                status: CollectorStatus::Failed,
                duration_ms,
                messages: vec![CollectionMessage {
                    code: failure.code.into(),
                    native_code: failure.native_code,
                    message: Some(failure.message.into()),
                }],
                fields: vec![],
            },
        },
    }
}

fn null_collection() -> CpuCollection {
    CpuCollection {
        architecture: None,
        topology: CpuTopology {
            physical_packages: None,
            physical_cores: None,
            logical_processors: None,
        },
        packages: None,
        features: CpuFeatures {
            available_instruction_sets: None,
            hardware_virtualization_supported: None,
            virtualization_firmware_enabled: None,
        },
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(windows)]
mod platform {
    use std::{
        collections::HashSet,
        io,
        mem::{offset_of, size_of},
        ptr,
    };

    use windows_sys::Win32::System::{
        SystemInformation::{
            GROUP_AFFINITY, GetLogicalProcessorInformationEx, GetNativeSystemInfo,
            PROCESSOR_ARCHITECTURE_AMD64, PROCESSOR_ARCHITECTURE_ARM, PROCESSOR_ARCHITECTURE_ARM64,
            PROCESSOR_ARCHITECTURE_INTEL, PROCESSOR_RELATIONSHIP, RelationProcessorCore,
            RelationProcessorPackage, SYSTEM_INFO, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
        },
        Threading::{
            GetCurrentThread, IsProcessorFeaturePresent, PF_ARM_NEON_INSTRUCTIONS_AVAILABLE,
            PF_ARM_V8_CRYPTO_INSTRUCTIONS_AVAILABLE, PF_VIRT_FIRMWARE_ENABLED,
            SetThreadGroupAffinity,
        },
    };

    use super::{
        CollectionFailure, CollectionMessage, CpuFeatures, CpuPackage, CpuSnapshot, CpuTopology,
        FieldCollectionStatus, MissingField, SystemArchitecture,
    };
    use pcdiag_core::CpuInstructionSet;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct PackageTopology {
        affinities: Vec<Affinity>,
        physical_cores: u32,
        logical_processors: u32,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Affinity {
        group: u16,
        mask: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CpuIdentity {
        manufacturer: String,
        model: String,
        instruction_sets: Vec<CpuInstructionSet>,
        hardware_virtualization_supported: bool,
    }

    pub(super) fn collect() -> Result<CpuSnapshot, CollectionFailure> {
        let architecture = architecture();
        let firmware_virtualization = firmware_virtualization_enabled();
        let package_affinities = processor_relationships(RelationProcessorPackage)?;
        let core_affinities = processor_relationships(RelationProcessorCore)?;
        if package_affinities.is_empty() {
            return Err(CollectionFailure {
                code: "cpu_package_enumeration_empty",
                native_code: None,
                message: "Windowsが物理CPUパッケージを報告しませんでした",
            });
        }
        if core_affinities.is_empty() {
            return Err(CollectionFailure {
                code: "cpu_core_enumeration_empty",
                native_code: None,
                message: "Windowsが物理CPUコアを報告しませんでした",
            });
        }

        let mut topologies = Vec::with_capacity(package_affinities.len());
        for package in package_affinities {
            let physical_cores = core_affinities
                .iter()
                .filter(|core| overlaps(&package, core))
                .count();
            let physical_cores = u32::try_from(physical_cores).map_err(|_| CollectionFailure {
                code: "cpu_topology_value_overflow",
                native_code: None,
                message: "CPUコア数をJSONの数値範囲で表現できませんでした",
            })?;
            let logical_processors = package
                .iter()
                .map(|affinity| affinity.mask.count_ones())
                .sum();
            if physical_cores == 0 || logical_processors == 0 {
                return Err(invalid_topology());
            }
            topologies.push(PackageTopology {
                affinities: package,
                physical_cores,
                logical_processors,
            });
        }

        let mut missing = Vec::new();
        let mut messages = Vec::new();
        let mut packages = Vec::with_capacity(topologies.len());
        let mut identities = Vec::with_capacity(topologies.len());
        for (index, topology) in topologies.iter().enumerate() {
            let package_index = u32::try_from(index).map_err(|_| CollectionFailure {
                code: "cpu_topology_value_overflow",
                native_code: None,
                message: "CPUパッケージ番号をJSONの数値範囲で表現できませんでした",
            })?;
            match identity_for_package(topology, topologies.len()) {
                Ok(identity) => {
                    packages.push(CpuPackage {
                        package_index,
                        manufacturer: Some(identity.manufacturer.clone()),
                        model: Some(identity.model.clone()),
                        physical_cores: Some(topology.physical_cores),
                        logical_processors: Some(topology.logical_processors),
                    });
                    identities.push(identity);
                }
                Err(failure) => {
                    messages.push(message(&failure));
                    for suffix in ["manufacturer", "model"] {
                        missing.push(MissingField {
                            path: format!("/cpu/packages/{index}/{suffix}"),
                            status: failure_status(&failure),
                            code: failure.code,
                            native_code: failure.native_code,
                        });
                    }
                    packages.push(CpuPackage {
                        package_index,
                        manufacturer: None,
                        model: None,
                        physical_cores: Some(topology.physical_cores),
                        logical_processors: Some(topology.logical_processors),
                    });
                }
            }
        }

        let (instruction_sets, hardware_virtualization_supported) =
            aggregate_features(&identities, topologies.len(), architecture, &mut missing);
        let physical_packages = u32::try_from(topologies.len()).map_err(|_| CollectionFailure {
            code: "cpu_topology_value_overflow",
            native_code: None,
            message: "物理CPU数をJSONの数値範囲で表現できませんでした",
        })?;
        let physical_cores = topologies
            .iter()
            .map(|package| package.physical_cores)
            .sum();
        let logical_processors = topologies
            .iter()
            .map(|package| package.logical_processors)
            .sum();

        Ok(CpuSnapshot {
            architecture: Some(architecture),
            topology: CpuTopology {
                physical_packages: Some(physical_packages),
                physical_cores: Some(physical_cores),
                logical_processors: Some(logical_processors),
            },
            packages: Some(packages),
            features: CpuFeatures {
                available_instruction_sets: instruction_sets,
                hardware_virtualization_supported,
                virtualization_firmware_enabled: Some(firmware_virtualization),
            },
            messages,
            missing,
        })
    }

    fn architecture() -> SystemArchitecture {
        let mut info = SYSTEM_INFO::default();
        // SAFETY: `info` points to writable SYSTEM_INFO storage.
        unsafe { GetNativeSystemInfo(&mut info) };
        // SAFETY: GetNativeSystemInfo initialized this union member.
        match unsafe { info.Anonymous.Anonymous.wProcessorArchitecture } {
            PROCESSOR_ARCHITECTURE_INTEL => SystemArchitecture::X86,
            PROCESSOR_ARCHITECTURE_AMD64 => SystemArchitecture::X86_64,
            PROCESSOR_ARCHITECTURE_ARM => SystemArchitecture::Arm,
            PROCESSOR_ARCHITECTURE_ARM64 => SystemArchitecture::Arm64,
            _ => SystemArchitecture::Unknown,
        }
    }

    fn firmware_virtualization_enabled() -> bool {
        // SAFETY: IsProcessorFeaturePresent has no pointer preconditions.
        unsafe { IsProcessorFeaturePresent(PF_VIRT_FIRMWARE_ENABLED) != 0 }
    }

    fn processor_relationships(relationship: i32) -> Result<Vec<Vec<Affinity>>, CollectionFailure> {
        let mut byte_len = 0;
        // SAFETY: null buffer with zero size requests the required length.
        unsafe { GetLogicalProcessorInformationEx(relationship, ptr::null_mut(), &mut byte_len) };
        if byte_len == 0 {
            return Err(last_error_failure(
                "cpu_topology_size_query_failed",
                "CPUトポロジーの必要バッファーサイズを取得できませんでした",
            ));
        }
        let word_size = size_of::<usize>();
        let word_len = usize::try_from(byte_len)
            .ok()
            .and_then(|length| length.checked_add(word_size - 1))
            .map(|length| length / word_size)
            .ok_or(CollectionFailure {
                code: "cpu_topology_buffer_overflow",
                native_code: None,
                message: "CPUトポロジー用バッファーを確保できませんでした",
            })?;
        let mut buffer = vec![0usize; word_len];
        // SAFETY: the buffer is aligned and has at least `byte_len` writable bytes.
        if unsafe {
            GetLogicalProcessorInformationEx(
                relationship,
                buffer
                    .as_mut_ptr()
                    .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>(),
                &mut byte_len,
            )
        } == 0
        {
            return Err(last_error_failure(
                "cpu_topology_query_failed",
                "WindowsからCPUトポロジーを取得できませんでした",
            ));
        }
        parse_relationships(
            buffer.as_ptr().cast::<u8>(),
            usize::try_from(byte_len).unwrap_or(0),
            relationship,
        )
    }

    fn parse_relationships(
        buffer: *const u8,
        byte_len: usize,
        expected_relationship: i32,
    ) -> Result<Vec<Vec<Affinity>>, CollectionFailure> {
        let mut result = Vec::new();
        let mut offset = 0usize;
        while offset < byte_len {
            let remaining = byte_len - offset;
            const HEADER_SIZE: usize = size_of::<i32>() + size_of::<u32>();
            if remaining < HEADER_SIZE {
                return Err(invalid_topology());
            }
            // SAFETY: bounds were checked and unaligned reads are used.
            let record_ptr = unsafe { buffer.add(offset) };
            // SAFETY: both fixed header fields are within the checked buffer.
            let record_relationship = unsafe { ptr::read_unaligned(record_ptr.cast::<i32>()) };
            // SAFETY: the size field immediately follows the relationship field.
            let record_size =
                unsafe { ptr::read_unaligned(record_ptr.add(size_of::<i32>()).cast::<u32>()) };
            let record_size = usize::try_from(record_size).map_err(|_| invalid_topology())?;
            if record_size == 0
                || record_size > remaining
                || record_relationship != expected_relationship
            {
                return Err(invalid_topology());
            }
            let processor_offset = offset_of!(SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, Anonymous);
            if processor_offset + size_of::<PROCESSOR_RELATIONSHIP>() > record_size {
                return Err(invalid_topology());
            }
            // SAFETY: the processor relationship's fixed portion is within this record.
            let processor_ptr = unsafe { record_ptr.add(processor_offset) };
            // SAFETY: bounds were checked and unaligned reads are used.
            let processor =
                unsafe { ptr::read_unaligned(processor_ptr.cast::<PROCESSOR_RELATIONSHIP>()) };
            let group_count = usize::from(processor.GroupCount);
            let groups_offset = processor_offset + offset_of!(PROCESSOR_RELATIONSHIP, GroupMask);
            let groups_bytes = group_count
                .checked_mul(size_of::<GROUP_AFFINITY>())
                .ok_or_else(invalid_topology)?;
            if group_count == 0 || groups_offset + groups_bytes > record_size {
                return Err(invalid_topology());
            }
            let mut groups = Vec::with_capacity(group_count);
            for index in 0..group_count {
                // SAFETY: the complete variable array was checked above; use an
                // unaligned read for each variable-length entry.
                let group = unsafe {
                    ptr::read_unaligned(
                        record_ptr
                            .add(groups_offset + index * size_of::<GROUP_AFFINITY>())
                            .cast::<GROUP_AFFINITY>(),
                    )
                };
                groups.push(Affinity {
                    group: group.Group,
                    mask: group.Mask,
                });
            }
            result.push(groups);
            offset = offset
                .checked_add(record_size)
                .ok_or_else(invalid_topology)?;
        }
        Ok(result)
    }

    fn overlaps(left: &[Affinity], right: &[Affinity]) -> bool {
        left.iter().any(|left| {
            right
                .iter()
                .any(|right| left.group == right.group && left.mask & right.mask != 0)
        })
    }

    fn identity_for_package(
        topology: &PackageTopology,
        package_count: usize,
    ) -> Result<CpuIdentity, CollectionFailure> {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if package_count == 1 {
                return x86_identity();
            }
            let affinity = topology
                .affinities
                .iter()
                .find(|affinity| affinity.mask != 0)
                .ok_or_else(invalid_topology)?;
            let target = GROUP_AFFINITY {
                Mask: 1usize << affinity.mask.trailing_zeros(),
                Group: affinity.group,
                Reserved: [0; 3],
            };
            let mut previous = GROUP_AFFINITY::default();
            // SAFETY: the pseudo-handle is valid for the current thread and both
            // affinity structures point to initialized storage.
            if unsafe { SetThreadGroupAffinity(GetCurrentThread(), &target, &mut previous) } == 0 {
                return Err(last_error_failure(
                    "cpu_package_affinity_failed",
                    "CPUパッケージを識別するためのスレッド割り当てに失敗しました",
                ));
            }
            let identity = x86_identity()?;
            let mut ignored = GROUP_AFFINITY::default();
            // SAFETY: restore the affinity returned by the successful prior call.
            if unsafe { SetThreadGroupAffinity(GetCurrentThread(), &previous, &mut ignored) } == 0 {
                return Err(last_error_failure(
                    "cpu_package_affinity_restore_failed",
                    "CPU識別後にスレッド割り当てを復元できませんでした",
                ));
            }
            Ok(identity)
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            let _ = (topology, package_count);
            Err(CollectionFailure {
                code: "cpu_identity_unsupported",
                native_code: None,
                message: "このCPUアーキテクチャではメーカーとモデルの取得に対応していません",
            })
        }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[derive(Debug, Clone, Copy)]
    struct CpuidValues {
        eax: u32,
        ebx: u32,
        ecx: u32,
        edx: u32,
    }

    #[cfg(target_arch = "x86")]
    #[allow(unused_unsafe)]
    fn cpuid(leaf: u32) -> CpuidValues {
        // SAFETY: CPUID is available on Windows-supported x86 processors. Some
        // Rust versions expose this intrinsic as safe, others as unsafe.
        let value = unsafe { std::arch::x86::__cpuid(leaf) };
        CpuidValues {
            eax: value.eax,
            ebx: value.ebx,
            ecx: value.ecx,
            edx: value.edx,
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[allow(unused_unsafe)]
    fn cpuid(leaf: u32) -> CpuidValues {
        // SAFETY: CPUID is always available on x86_64. The allow keeps this
        // source compatible with Rust versions where the intrinsic is safe.
        let value = unsafe { std::arch::x86_64::__cpuid(leaf) };
        CpuidValues {
            eax: value.eax,
            ebx: value.ebx,
            ecx: value.ecx,
            edx: value.edx,
        }
    }

    #[cfg(target_arch = "x86")]
    #[allow(unused_unsafe)]
    fn cpuid_count(leaf: u32, subleaf: u32) -> CpuidValues {
        // SAFETY: callers check the maximum supported leaf before this call.
        let value = unsafe { std::arch::x86::__cpuid_count(leaf, subleaf) };
        CpuidValues {
            eax: value.eax,
            ebx: value.ebx,
            ecx: value.ecx,
            edx: value.edx,
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[allow(unused_unsafe)]
    fn cpuid_count(leaf: u32, subleaf: u32) -> CpuidValues {
        // SAFETY: callers check the maximum supported leaf before this call.
        let value = unsafe { std::arch::x86_64::__cpuid_count(leaf, subleaf) };
        CpuidValues {
            eax: value.eax,
            ebx: value.ebx,
            ecx: value.ecx,
            edx: value.edx,
        }
    }

    #[cfg(target_arch = "x86")]
    fn xgetbv() -> u64 {
        // SAFETY: called only when CPUID reports OSXSAVE.
        unsafe { std::arch::x86::_xgetbv(0) }
    }

    #[cfg(target_arch = "x86_64")]
    fn xgetbv() -> u64 {
        // SAFETY: called only when CPUID reports OSXSAVE.
        unsafe { std::arch::x86_64::_xgetbv(0) }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn x86_identity() -> Result<CpuIdentity, CollectionFailure> {
        let basic = cpuid(0);
        let mut vendor = Vec::with_capacity(12);
        vendor.extend_from_slice(&basic.ebx.to_le_bytes());
        vendor.extend_from_slice(&basic.edx.to_le_bytes());
        vendor.extend_from_slice(&basic.ecx.to_le_bytes());
        let manufacturer = String::from_utf8_lossy(&vendor).trim().to_owned();

        let extended = cpuid(0x8000_0000);
        let mut brand = Vec::with_capacity(48);
        if extended.eax >= 0x8000_0004 {
            for leaf in 0x8000_0002..=0x8000_0004 {
                let value = cpuid(leaf);
                for register in [value.eax, value.ebx, value.ecx, value.edx] {
                    brand.extend_from_slice(&register.to_le_bytes());
                }
            }
        }
        let model = String::from_utf8_lossy(&brand)
            .trim_matches(char::from(0))
            .trim()
            .to_owned();
        if manufacturer.is_empty() || model.is_empty() {
            return Err(CollectionFailure {
                code: "cpu_identity_unavailable",
                native_code: None,
                message: "CPUIDからCPUメーカーまたはモデル名を取得できませんでした",
            });
        }

        let leaf1 = cpuid(1);
        let os_avx = leaf1.ecx & (1 << 27) != 0 && xgetbv() & 0b110 == 0b110;
        let mut instruction_sets = Vec::new();
        for (available, instruction_set) in [
            (leaf1.edx & (1 << 26) != 0, CpuInstructionSet::Sse2),
            (leaf1.ecx & 1 != 0, CpuInstructionSet::Sse3),
            (leaf1.ecx & (1 << 9) != 0, CpuInstructionSet::Ssse3),
            (leaf1.ecx & (1 << 19) != 0, CpuInstructionSet::Sse4_1),
            (leaf1.ecx & (1 << 20) != 0, CpuInstructionSet::Sse4_2),
            (leaf1.ecx & (1 << 28) != 0 && os_avx, CpuInstructionSet::Avx),
            (leaf1.ecx & (1 << 25) != 0, CpuInstructionSet::Aes),
        ] {
            if available {
                instruction_sets.push(instruction_set);
            }
        }
        if basic.eax >= 7 {
            let leaf7 = cpuid_count(7, 0);
            if leaf7.ebx & (1 << 5) != 0 && os_avx {
                instruction_sets.push(CpuInstructionSet::Avx2);
            }
            if leaf7.ebx & (1 << 29) != 0 {
                instruction_sets.push(CpuInstructionSet::Sha);
            }
        }
        let hardware_virtualization_supported = if manufacturer == "GenuineIntel" {
            leaf1.ecx & (1 << 5) != 0
        } else if manufacturer == "AuthenticAMD" && extended.eax >= 0x8000_0001 {
            cpuid(0x8000_0001).ecx & (1 << 2) != 0
        } else {
            false
        };
        Ok(CpuIdentity {
            manufacturer,
            model,
            instruction_sets,
            hardware_virtualization_supported,
        })
    }

    fn aggregate_features(
        identities: &[CpuIdentity],
        package_count: usize,
        architecture: SystemArchitecture,
        missing: &mut Vec<MissingField>,
    ) -> (Option<Vec<CpuInstructionSet>>, Option<bool>) {
        if identities.len() == package_count {
            let mut available: HashSet<_> =
                identities[0].instruction_sets.iter().copied().collect();
            for identity in &identities[1..] {
                let current: HashSet<_> = identity.instruction_sets.iter().copied().collect();
                available.retain(|feature| current.contains(feature));
            }
            let ordered = [
                CpuInstructionSet::Sse2,
                CpuInstructionSet::Sse3,
                CpuInstructionSet::Ssse3,
                CpuInstructionSet::Sse4_1,
                CpuInstructionSet::Sse4_2,
                CpuInstructionSet::Avx,
                CpuInstructionSet::Avx2,
                CpuInstructionSet::Aes,
                CpuInstructionSet::Sha,
                CpuInstructionSet::Neon,
                CpuInstructionSet::ArmV8Crypto,
            ];
            return (
                Some(
                    ordered
                        .into_iter()
                        .filter(|feature| available.contains(feature))
                        .collect(),
                ),
                Some(
                    identities
                        .iter()
                        .all(|identity| identity.hardware_virtualization_supported),
                ),
            );
        }

        if matches!(
            architecture,
            SystemArchitecture::Arm | SystemArchitecture::Arm64
        ) {
            let mut instruction_sets = Vec::new();
            // SAFETY: IsProcessorFeaturePresent has no pointer preconditions.
            if unsafe { IsProcessorFeaturePresent(PF_ARM_NEON_INSTRUCTIONS_AVAILABLE) } != 0 {
                instruction_sets.push(CpuInstructionSet::Neon);
            }
            // SAFETY: IsProcessorFeaturePresent has no pointer preconditions.
            if unsafe { IsProcessorFeaturePresent(PF_ARM_V8_CRYPTO_INSTRUCTIONS_AVAILABLE) } != 0 {
                instruction_sets.push(CpuInstructionSet::ArmV8Crypto);
            }
            missing.push(MissingField {
                path: "/cpu/features/hardware_virtualization_supported".into(),
                status: FieldCollectionStatus::Unsupported,
                code: "cpu_virtualization_support_unsupported",
                native_code: None,
            });
            return (Some(instruction_sets), None);
        }

        for path in [
            "/cpu/features/available_instruction_sets",
            "/cpu/features/hardware_virtualization_supported",
        ] {
            missing.push(MissingField {
                path: path.into(),
                status: FieldCollectionStatus::Unsupported,
                code: "cpu_feature_query_incomplete",
                native_code: None,
            });
        }
        (None, None)
    }

    fn failure_status(failure: &CollectionFailure) -> FieldCollectionStatus {
        if failure.code.ends_with("unsupported") {
            FieldCollectionStatus::Unsupported
        } else {
            FieldCollectionStatus::Failed
        }
    }

    fn message(failure: &CollectionFailure) -> CollectionMessage {
        CollectionMessage {
            code: failure.code.into(),
            native_code: failure.native_code,
            message: Some(failure.message.into()),
        }
    }

    fn invalid_topology() -> CollectionFailure {
        CollectionFailure {
            code: "cpu_topology_invalid",
            native_code: None,
            message: "Windowsが返したCPUトポロジーを解釈できませんでした",
        }
    }

    fn last_error_failure(code: &'static str, message: &'static str) -> CollectionFailure {
        CollectionFailure {
            code,
            native_code: io::Error::last_os_error().raw_os_error().map(i64::from),
            message,
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{CollectionFailure, CpuSnapshot};

    pub(super) fn collect() -> Result<CpuSnapshot, CollectionFailure> {
        Err(CollectionFailure {
            code: "platform_not_supported",
            native_code: None,
            message: "Windows以外の環境ではCPU情報を収集できません",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcdiag_core::CpuInstructionSet;

    #[test]
    fn maps_complete_cpu_snapshot() {
        let snapshot = CpuSnapshot {
            architecture: Some(SystemArchitecture::X86_64),
            topology: CpuTopology {
                physical_packages: Some(1),
                physical_cores: Some(8),
                logical_processors: Some(16),
            },
            packages: Some(vec![CpuPackage {
                package_index: 0,
                manufacturer: Some("GenuineIntel".into()),
                model: Some("Example CPU".into()),
                physical_cores: Some(8),
                logical_processors: Some(16),
            }]),
            features: CpuFeatures {
                available_instruction_sets: Some(vec![CpuInstructionSet::Sse2]),
                hardware_virtualization_supported: Some(true),
                virtualization_firmware_enabled: Some(true),
            },
            messages: vec![],
            missing: vec![],
        };

        let result = build_result(Ok(snapshot), 3);

        assert_eq!(result.status.status, CollectorStatus::Success);
        assert_eq!(result.collection.topology.physical_cores, Some(8));
    }

    #[test]
    fn missing_identity_keeps_topology_and_makes_result_partial() {
        let snapshot = CpuSnapshot {
            architecture: Some(SystemArchitecture::Arm64),
            topology: CpuTopology {
                physical_packages: Some(1),
                physical_cores: Some(8),
                logical_processors: Some(8),
            },
            packages: Some(vec![CpuPackage {
                package_index: 0,
                manufacturer: None,
                model: None,
                physical_cores: Some(8),
                logical_processors: Some(8),
            }]),
            features: CpuFeatures {
                available_instruction_sets: Some(vec![CpuInstructionSet::Neon]),
                hardware_virtualization_supported: None,
                virtualization_firmware_enabled: Some(true),
            },
            messages: vec![],
            missing: vec![MissingField {
                path: "/cpu/packages/0/model".into(),
                status: FieldCollectionStatus::Unsupported,
                code: "cpu_identity_unsupported",
                native_code: None,
            }],
        };

        let result = build_result(Ok(snapshot), 1);

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(result.collection.topology.physical_cores, Some(8));
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_collection_returns_failed() {
        let result = collect_cpu();

        assert_eq!(result.status.status, CollectorStatus::Failed);
        assert!(result.collection.packages.is_none());
    }
}
