use std::{
    collections::{BTreeMap, HashSet},
    fmt,
};

use serde_json::Value;

use crate::{
    Collection, CollectionStatus, CollectorName, CollectorResult, CollectorStatus, Diagnosis,
    Evidence, RuleEvaluationStatus, Severity,
};

const MEMORY_PATHS: [&str; 7] = [
    "/memory/physical/total_bytes",
    "/memory/physical/available_bytes",
    "/memory/physical/load_percent",
    "/memory/commit/limit_bytes",
    "/memory/commit/available_bytes",
    "/memory/virtual/total_bytes",
    "/memory/virtual/available_bytes",
];

const WINDOWS_PATHS: [&str; 7] = [
    "/windows/edition",
    "/windows/version",
    "/windows/build_number",
    "/windows/architecture",
    "/windows/booted_at",
    "/windows/uptime_ms",
    "/windows/boot_mode",
];

const CLOCK_PATHS: [&str; 4] = [
    "/clock/system_time_utc",
    "/clock/utc_offset_minutes",
    "/clock/windows_time_service",
    "/clock/hardware_clock",
];

const CPU_FAILURE_PATHS: [&str; 9] = [
    "/cpu/architecture",
    "/cpu/topology/physical_packages",
    "/cpu/topology/physical_cores",
    "/cpu/topology/logical_processors",
    "/cpu/packages",
    "/cpu/features/available_instruction_sets",
    "/cpu/features/hardware_virtualization_extensions_available",
    "/cpu/features/virtualization_firmware_enabled",
    "/cpu/features/hypervisor_present",
];

const FIRMWARE_PATHS: [&str; 6] = [
    "/firmware/vendor",
    "/firmware/version",
    "/firmware/release_date",
    "/firmware/interface_type",
    "/firmware/secure_boot_enabled",
    "/firmware/status",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationErrors(Vec<ValidationError>);

impl ValidationErrors {
    pub fn errors(&self) -> &[ValidationError] {
        &self.0
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} validation error(s)", self.0.len())
    }
}

impl std::error::Error for ValidationErrors {}

impl Collection {
    pub fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = Vec::new();
        if self.windows.version.as_deref() == Some("") {
            push_error(&mut errors, "/windows/version", "must not be empty");
        }
        if self.windows.build_number == Some(0) {
            push_error(
                &mut errors,
                "/windows/build_number",
                "must be greater than zero",
            );
        }
        if let Some(booted_at) = &self.windows.booted_at
            && (!booted_at.contains('T') || !booted_at.ends_with('Z'))
        {
            push_error(
                &mut errors,
                "/windows/booted_at",
                "must be a UTC date-time ending in Z",
            );
        }
        if self.windows_updates.lookback_days == Some(0) {
            push_error(
                &mut errors,
                "/windows_updates/lookback_days",
                "must be greater than zero or null for all history",
            );
        }
        if self.windows_updates.max_entries == Some(0) {
            push_error(
                &mut errors,
                "/windows_updates/max_entries",
                "must be greater than zero or null for all history",
            );
        }
        if let Some(history) = &self.windows_updates.history {
            for (index, entry) in history.iter().enumerate() {
                let base = format!("/windows_updates/history/{index}");
                if !entry.occurred_at.contains('T') || !entry.occurred_at.ends_with('Z') {
                    push_error(
                        &mut errors,
                        format!("{base}/occurred_at"),
                        "must be a UTC date-time ending in Z",
                    );
                }
                if entry.title.as_deref() == Some("") {
                    push_error(&mut errors, format!("{base}/title"), "must not be empty");
                }
                let mut kb_ids = HashSet::new();
                for (kb_index, kb_id) in entry.kb_ids.iter().enumerate() {
                    if !kb_id.starts_with("KB")
                        || kb_id.len() < 8
                        || !kb_id[2..].bytes().all(|value| value.is_ascii_digit())
                    {
                        push_error(
                            &mut errors,
                            format!("{base}/kb_ids/{kb_index}"),
                            "must be an uppercase KB identifier",
                        );
                    }
                    if !kb_ids.insert(kb_id) {
                        push_error(
                            &mut errors,
                            format!("{base}/kb_ids/{kb_index}"),
                            "must be unique within the history entry",
                        );
                    }
                }
            }
        }
        if let Some(system_time) = &self.clock.system_time_utc
            && (!system_time.contains('T') || !system_time.ends_with('Z'))
        {
            push_error(
                &mut errors,
                "/clock/system_time_utc",
                "must be a UTC date-time ending in Z",
            );
        }
        if self
            .clock
            .utc_offset_minutes
            .is_some_and(|offset| !(-1_440..=1_440).contains(&offset))
        {
            push_error(
                &mut errors,
                "/clock/utc_offset_minutes",
                "must be between -1440 and 1440",
            );
        }
        if let Some(hardware_clock) = &self.clock.hardware_clock
            && let Some(time) = &hardware_clock.time_utc
            && (!time.contains('T') || !time.ends_with('Z'))
        {
            push_error(
                &mut errors,
                "/clock/hardware_clock/time_utc",
                "must be a UTC date-time ending in Z",
            );
        }
        for (path, value) in [
            ("/firmware/vendor", self.firmware.vendor.as_deref()),
            ("/firmware/version", self.firmware.version.as_deref()),
        ] {
            if value == Some("") {
                push_error(&mut errors, path, "must not be empty");
            }
        }
        if let Some(release_date) = &self.firmware.release_date
            && !is_iso_date(release_date)
        {
            push_error(
                &mut errors,
                "/firmware/release_date",
                "must be a valid YYYY-MM-DD date",
            );
        }
        if self.firmware.interface_type == Some(crate::FirmwareInterfaceType::Bios)
            && self.firmware.secure_boot_enabled.is_some()
        {
            push_error(
                &mut errors,
                "/firmware/secure_boot_enabled",
                "must be null when interface_type is bios",
            );
        }
        let topology = &self.cpu.topology;
        for (path, value) in [
            (
                "/cpu/topology/physical_packages",
                topology.physical_packages,
            ),
            ("/cpu/topology/physical_cores", topology.physical_cores),
            (
                "/cpu/topology/logical_processors",
                topology.logical_processors,
            ),
        ] {
            if value == Some(0) {
                push_error(&mut errors, path, "must be greater than zero");
            }
        }
        if let (Some(packages), Some(cores)) = (topology.physical_packages, topology.physical_cores)
            && cores < packages
        {
            push_error(
                &mut errors,
                "/cpu/topology/physical_cores",
                "must not be less than physical_packages",
            );
        }
        if let (Some(cores), Some(logical)) = (topology.physical_cores, topology.logical_processors)
            && logical < cores
        {
            push_error(
                &mut errors,
                "/cpu/topology/logical_processors",
                "must not be less than physical_cores",
            );
        }
        if let Some(packages) = &self.cpu.packages {
            if let Some(expected) = topology.physical_packages
                && usize::try_from(expected).ok() != Some(packages.len())
            {
                push_error(
                    &mut errors,
                    "/cpu/topology/physical_packages",
                    "must match the number of CPU packages",
                );
            }
            let mut indexes = HashSet::new();
            for (index, package) in packages.iter().enumerate() {
                if !indexes.insert(package.package_index) {
                    push_error(
                        &mut errors,
                        format!("/cpu/packages/{index}/package_index"),
                        "must be unique within the CPU package collection",
                    );
                }
                if package.manufacturer.as_deref() == Some("") {
                    push_error(
                        &mut errors,
                        format!("/cpu/packages/{index}/manufacturer"),
                        "must not be empty",
                    );
                }
                if package.model.as_deref() == Some("") {
                    push_error(
                        &mut errors,
                        format!("/cpu/packages/{index}/model"),
                        "must not be empty",
                    );
                }
                if package.physical_cores == Some(0) {
                    push_error(
                        &mut errors,
                        format!("/cpu/packages/{index}/physical_cores"),
                        "must be greater than zero",
                    );
                }
                if package.logical_processors == Some(0) {
                    push_error(
                        &mut errors,
                        format!("/cpu/packages/{index}/logical_processors"),
                        "must be greater than zero",
                    );
                }
                if let (Some(cores), Some(logical)) =
                    (package.physical_cores, package.logical_processors)
                    && logical < cores
                {
                    push_error(
                        &mut errors,
                        format!("/cpu/packages/{index}/logical_processors"),
                        "must not be less than physical_cores",
                    );
                }
            }
            if packages
                .iter()
                .all(|package| package.physical_cores.is_some())
                && let Some(total) = topology.physical_cores
                && packages
                    .iter()
                    .map(|package| u64::from(package.physical_cores.unwrap_or(0)))
                    .sum::<u64>()
                    != u64::from(total)
            {
                push_error(
                    &mut errors,
                    "/cpu/topology/physical_cores",
                    "must equal the sum of package physical cores",
                );
            }
            if packages
                .iter()
                .all(|package| package.logical_processors.is_some())
                && let Some(total) = topology.logical_processors
                && packages
                    .iter()
                    .map(|package| u64::from(package.logical_processors.unwrap_or(0)))
                    .sum::<u64>()
                    != u64::from(total)
            {
                push_error(
                    &mut errors,
                    "/cpu/topology/logical_processors",
                    "must equal the sum of package logical processors",
                );
            }
        }
        if let Some(instruction_sets) = &self.cpu.features.available_instruction_sets {
            let mut unique = HashSet::new();
            for instruction_set in instruction_sets {
                if !unique.insert(instruction_set) {
                    push_error(
                        &mut errors,
                        "/cpu/features/available_instruction_sets",
                        "must not contain duplicate instruction sets",
                    );
                    break;
                }
            }
        }
        let physical = &self.memory.physical;

        if let Some(load) = physical.load_percent
            && (!load.is_finite() || !(0.0..=100.0).contains(&load))
        {
            push_error(
                &mut errors,
                "/memory/physical/load_percent",
                "must be a finite number between 0 and 100",
            );
        }

        validate_available_not_greater_than_total(
            &mut errors,
            "/memory/physical",
            physical.available_bytes,
            physical.total_bytes,
        );

        if let Some(gpus) = &self.gpus {
            let mut instance_ids = HashSet::new();
            for (index, gpu) in gpus.iter().enumerate() {
                if let Some(instance_id) = &gpu.device_instance_id
                    && !instance_ids.insert(instance_id)
                {
                    push_error(
                        &mut errors,
                        format!("/gpus/{index}/device_instance_id"),
                        "must be unique within the GPU collection",
                    );
                }
            }
        }
        if let Some(devices) = &self.devices {
            let mut instance_ids = HashSet::new();
            for (index, device) in devices.iter().enumerate() {
                if let Some(instance_id) = &device.device_instance_id
                    && !instance_ids.insert(instance_id)
                {
                    push_error(
                        &mut errors,
                        format!("/devices/{index}/device_instance_id"),
                        "must be unique within the device collection",
                    );
                }
            }
        }
        if let Some(disks) = &self.storage.disks {
            let mut numbers = HashSet::new();
            for (index, disk) in disks.iter().enumerate() {
                if !numbers.insert(disk.number) {
                    push_error(
                        &mut errors,
                        format!("/storage/disks/{index}/number"),
                        "must be unique within the physical disk collection",
                    );
                }
                if disk.capacity_bytes == Some(0) {
                    push_error(
                        &mut errors,
                        format!("/storage/disks/{index}/capacity_bytes"),
                        "must be greater than zero",
                    );
                }
                if disk.logical_sector_size_bytes == Some(0) {
                    push_error(
                        &mut errors,
                        format!("/storage/disks/{index}/logical_sector_size_bytes"),
                        "must be greater than zero",
                    );
                }
            }
        }
        if let Some(partitions) = &self.storage.partitions {
            let mut identities = HashSet::new();
            let mut ranges: BTreeMap<u32, Vec<(usize, u64, u64)>> = BTreeMap::new();
            for (index, partition) in partitions.iter().enumerate() {
                if !identities.insert((partition.disk_number, partition.partition_number)) {
                    push_error(
                        &mut errors,
                        format!("/storage/partitions/{index}/partition_number"),
                        "disk number and partition number must be unique",
                    );
                }
                if partition.length_bytes == 0 {
                    push_error(
                        &mut errors,
                        format!("/storage/partitions/{index}/length_bytes"),
                        "must be greater than zero",
                    );
                }
                validate_storage_range(
                    &mut errors,
                    &format!("/storage/partitions/{index}"),
                    partition.disk_number,
                    partition.offset_bytes,
                    partition.length_bytes,
                    self.storage.disks.as_deref(),
                );
                if !is_mbr_extended_partition(partition)
                    && let Some(end) = partition.offset_bytes.checked_add(partition.length_bytes)
                {
                    ranges.entry(partition.disk_number).or_default().push((
                        index,
                        partition.offset_bytes,
                        end,
                    ));
                }
            }
            for mut disk_ranges in ranges.into_values() {
                disk_ranges.sort_by_key(|(_, offset, _)| *offset);
                let mut furthest_end = 0;
                for (index, offset, end) in disk_ranges {
                    if offset < furthest_end {
                        push_error(
                            &mut errors,
                            format!("/storage/partitions/{index}/offset_bytes"),
                            "partition range must not overlap another partition on the same disk",
                        );
                    }
                    furthest_end = furthest_end.max(end);
                }
            }
        }
        if let Some(volumes) = &self.storage.volumes {
            let mut mount_points = HashSet::new();
            for (index, volume) in volumes.iter().enumerate() {
                if let (Some(free), Some(capacity)) = (volume.free_bytes, volume.capacity_bytes)
                    && free > capacity
                {
                    push_error(
                        &mut errors,
                        format!("/storage/volumes/{index}/free_bytes"),
                        "must not be greater than capacity_bytes",
                    );
                }
                if let Some(volume_mount_points) = &volume.mount_points {
                    for mount_point in volume_mount_points {
                        if !mount_points.insert(mount_point) {
                            push_error(
                                &mut errors,
                                format!("/storage/volumes/{index}/mount_points"),
                                "mount points must be unique within the volume collection",
                            );
                        }
                    }
                }
                if let Some(extents) = &volume.extents {
                    for (extent_index, extent) in extents.iter().enumerate() {
                        let base = format!("/storage/volumes/{index}/extents/{extent_index}");
                        if extent.length_bytes == 0 {
                            push_error(
                                &mut errors,
                                format!("{base}/length_bytes"),
                                "must be greater than zero",
                            );
                        }
                        validate_storage_range(
                            &mut errors,
                            &base,
                            extent.disk_number,
                            extent.offset_bytes,
                            extent.length_bytes,
                            self.storage.disks.as_deref(),
                        );
                    }
                }
            }
        }
        if let Some(smart_values) = &self.storage.smart {
            let mut disk_numbers = HashSet::new();
            for (index, smart) in smart_values.iter().enumerate() {
                if !disk_numbers.insert(smart.disk_number) {
                    push_error(
                        &mut errors,
                        format!("/storage/smart/{index}/disk_number"),
                        "must be unique within the SMART collection",
                    );
                }
                if let Some(disks) = &self.storage.disks
                    && !disks.iter().any(|disk| disk.number == smart.disk_number)
                {
                    push_error(
                        &mut errors,
                        format!("/storage/smart/{index}/disk_number"),
                        "must refer to a collected physical disk",
                    );
                }
                if smart
                    .available_spare_percent
                    .is_some_and(|value| value > 100)
                {
                    push_error(
                        &mut errors,
                        format!("/storage/smart/{index}/available_spare_percent"),
                        "must not be greater than 100",
                    );
                }
            }
        }
        validate_available_not_greater_than_total(
            &mut errors,
            "/memory/commit",
            self.memory.commit.available_bytes,
            self.memory.commit.limit_bytes,
        );
        validate_available_not_greater_than_total(
            &mut errors,
            "/memory/virtual",
            self.memory.virtual_memory.available_bytes,
            self.memory.virtual_memory.total_bytes,
        );
        if !(1..=3_650).contains(&self.event_logs.lookback_days) {
            push_error(
                &mut errors,
                "/event_logs/lookback_days",
                "must be between 1 and 3650",
            );
        }
        for (field, expected_log, events) in [
            ("system", "System", self.event_logs.system.as_deref()),
            (
                "application",
                "Application",
                self.event_logs.application.as_deref(),
            ),
            ("security", "Security", self.event_logs.security.as_deref()),
        ] {
            if let Some(events) = events {
                for (index, event) in events.iter().enumerate() {
                    let base = format!("/event_logs/{field}/{index}");
                    if event.log_name != expected_log {
                        push_error(
                            &mut errors,
                            format!("{base}/log_name"),
                            format!("must be {expected_log}"),
                        );
                    }
                    if !event.occurred_at.contains('T') {
                        push_error(
                            &mut errors,
                            format!("{base}/occurred_at"),
                            "must be an ISO date-time",
                        );
                    }
                    if event.provider.is_empty() || event.summary.is_empty() {
                        push_error(&mut errors, base, "provider and summary must not be empty");
                    }
                }
            }
        }

        finish(errors)
    }

    pub fn validate_with_status(&self, status: &CollectionStatus) -> Result<(), ValidationErrors> {
        let mut errors = self.validate().err().map_or_else(Vec::new, |e| e.0);
        let windows_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::Windows)
            .collect();
        if windows_collectors.len() > 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must not contain more than one Windows collector result",
            );
        } else if let Some(collector) = windows_collectors.first() {
            validate_windows_status(self, collector, &mut errors);
        }
        let windows_update_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::WindowsUpdates)
            .collect();
        if windows_update_collectors.len() > 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must not contain more than one Windows Update collector result",
            );
        } else if let Some(collector) = windows_update_collectors.first() {
            validate_windows_update_status(self, collector, &mut errors);
        }
        let clock_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::Clock)
            .collect();
        if clock_collectors.len() > 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must not contain more than one clock collector result",
            );
        } else if let Some(collector) = clock_collectors.first() {
            validate_clock_status(self, collector, &mut errors);
        }
        let cpu_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::Cpu)
            .collect();
        if cpu_collectors.len() > 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must not contain more than one CPU collector result",
            );
        } else if let Some(collector) = cpu_collectors.first() {
            validate_cpu_status(self, collector, &mut errors);
        }
        let firmware_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::Firmware)
            .collect();
        if firmware_collectors.len() > 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must not contain more than one firmware collector result",
            );
        } else if let Some(collector) = firmware_collectors.first() {
            validate_firmware_status(self, collector, &mut errors);
        }
        let memory_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::Memory)
            .collect();

        if memory_collectors.len() != 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must contain exactly one memory collector result",
            );
            return finish(errors);
        }

        validate_memory_status(self, memory_collectors[0], &mut errors);
        let gpu_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::Gpu)
            .collect();
        if gpu_collectors.len() > 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must not contain more than one GPU collector result",
            );
        } else if let Some(collector) = gpu_collectors.first() {
            validate_gpu_status(self, collector, &mut errors);
        }
        let device_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::Devices)
            .collect();
        if device_collectors.len() > 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must not contain more than one device collector result",
            );
        } else if let Some(collector) = device_collectors.first() {
            validate_device_status(self, collector, &mut errors);
        }
        let disk_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::PhysicalDisks)
            .collect();
        if disk_collectors.len() > 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must not contain more than one physical disk collector result",
            );
        } else if let Some(collector) = disk_collectors.first() {
            validate_physical_disk_status(self, collector, &mut errors);
        }
        validate_single_storage_collector(
            self,
            status,
            CollectorName::Partitions,
            "/storage/partitions",
            self.storage.partitions.is_some(),
            true,
            &mut errors,
        );
        validate_single_storage_collector(
            self,
            status,
            CollectorName::Smart,
            "/storage/smart",
            self.storage.smart.is_some(),
            true,
            &mut errors,
        );
        validate_single_storage_collector(
            self,
            status,
            CollectorName::Volumes,
            "/storage/volumes",
            self.storage.volumes.is_some(),
            false,
            &mut errors,
        );
        let event_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::EventLogs)
            .collect();
        if event_collectors.len() > 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must not contain more than one event log collector result",
            );
        } else if let Some(collector) = event_collectors.first() {
            let missing = [
                ("/event_logs/system", self.event_logs.system.is_none()),
                (
                    "/event_logs/application",
                    self.event_logs.application.is_none(),
                ),
                ("/event_logs/security", self.event_logs.security.is_none()),
            ];
            match collector.status {
                CollectorStatus::Success => {
                    if missing.iter().any(|(_, value)| *value)
                        || !collector.fields.is_empty()
                        || !collector.messages.is_empty()
                    {
                        push_error(
                            &mut errors,
                            "/collectors/event_logs/status",
                            "successful event log collector must not contain failures",
                        );
                    }
                }
                CollectorStatus::Partial => {
                    for (path, is_missing) in missing {
                        if is_missing && !collector.fields.iter().any(|field| field.path == path) {
                            push_error(&mut errors, path, "missing log requires a field result");
                        }
                    }
                }
                CollectorStatus::Skipped | CollectorStatus::Failed => {
                    if missing.iter().any(|(_, value)| !*value) {
                        push_error(
                            &mut errors,
                            "/event_logs",
                            "skipped or failed event log collector requires all logs to be null",
                        );
                    }
                    if collector.messages.is_empty() {
                        push_error(
                            &mut errors,
                            "/collectors/event_logs/messages",
                            "skipped or failed event log collector must include a reason",
                        );
                    }
                }
            }
        }
        finish(errors)
    }
}

fn validate_windows_update_status(
    collection: &Collection,
    collector: &CollectorResult,
    errors: &mut Vec<ValidationError>,
) {
    match collector.status {
        CollectorStatus::Success => {
            if collection.windows_updates.history.is_none() || !collector.fields.is_empty() {
                push_error(
                    errors,
                    "/collectors/windows_updates/status",
                    "successful collector requires a history array and no field failures",
                );
            }
            if collector.messages.iter().any(|message| {
                !matches!(
                    message.code.as_str(),
                    "windows_update_history_truncated_by_date"
                        | "windows_update_history_truncated_by_count"
                )
            }) {
                push_error(
                    errors,
                    "/collectors/windows_updates/messages",
                    "successful collector messages must only describe configured truncation",
                );
            }
        }
        CollectorStatus::Partial => {
            if collection.windows_updates.history.is_none() || collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/windows_updates/status",
                    "partial collector requires collected history and a reason",
                );
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if collection.windows_updates.history.is_some() || collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/windows_updates/status",
                    "skipped or failed collector requires null history and a reason",
                );
            }
        }
    }
    for field in &collector.fields {
        if field.path != "/windows_updates/history" {
            push_error(
                errors,
                &field.path,
                "Windows Update field result refers to an unknown path",
            );
        }
    }
}

fn validate_firmware_status(
    collection: &Collection,
    collector: &CollectorResult,
    errors: &mut Vec<ValidationError>,
) {
    let value = serde_json::to_value(&collection.firmware).expect("firmware model is serializable");
    match collector.status {
        CollectorStatus::Success | CollectorStatus::Partial => {
            for field in &collector.fields {
                validate_null_field_result(&value, field, "/firmware", errors);
            }
            for path in FIRMWARE_PATHS {
                let relative = path.strip_prefix("/firmware").unwrap_or(path);
                if value.pointer(relative).is_some_and(Value::is_null)
                    && !collector.fields.iter().any(|field| field.path == path)
                {
                    push_error(
                        errors,
                        path,
                        "null value requires a field collection result",
                    );
                }
            }
            if collector.status == CollectorStatus::Success
                && (!collector.fields.is_empty() || !collector.messages.is_empty())
            {
                push_error(
                    errors,
                    "/collectors/firmware/status",
                    "successful firmware collector must not contain failures",
                );
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if FIRMWARE_PATHS.iter().any(|path| {
                let relative = path.strip_prefix("/firmware").unwrap_or(path);
                value
                    .pointer(relative)
                    .is_some_and(|value| !value.is_null())
            }) {
                push_error(
                    errors,
                    "/firmware",
                    "skipped or failed firmware collector requires all values to be null",
                );
            }
            if collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/firmware/messages",
                    "skipped or failed firmware collector must include a reason",
                );
            }
        }
    }
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let Ok(year) = value[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    year > 0 && (1..=12).contains(&month) && day > 0 && day <= days_in_month(year, month)
}

#[allow(clippy::manual_is_multiple_of)]
fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        _ => 31,
    }
}

fn validate_clock_status(
    collection: &Collection,
    collector: &CollectorResult,
    errors: &mut Vec<ValidationError>,
) {
    let value = serde_json::to_value(&collection.clock).expect("clock model is serializable");
    match collector.status {
        CollectorStatus::Success | CollectorStatus::Partial => {
            for field in &collector.fields {
                validate_null_field_result(&value, field, "/clock", errors);
            }
            for path in CLOCK_PATHS {
                let relative = path.strip_prefix("/clock").unwrap_or(path);
                if value.pointer(relative).is_some_and(Value::is_null)
                    && !collector.fields.iter().any(|field| field.path == path)
                {
                    push_error(
                        errors,
                        path,
                        "null value requires a field collection result",
                    );
                }
            }
            if collector.status == CollectorStatus::Success
                && (!collector.fields.is_empty() || !collector.messages.is_empty())
            {
                push_error(
                    errors,
                    "/collectors/clock/status",
                    "successful clock collector must not contain failures",
                );
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if CLOCK_PATHS.iter().any(|path| {
                let relative = path.strip_prefix("/clock").unwrap_or(path);
                value
                    .pointer(relative)
                    .is_some_and(|value| !value.is_null())
            }) {
                push_error(
                    errors,
                    "/clock",
                    "skipped or failed clock collector requires all values to be null",
                );
            }
            if collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/clock/messages",
                    "skipped or failed clock collector must include a reason",
                );
            }
        }
    }
}

fn validate_cpu_status(
    collection: &Collection,
    collector: &CollectorResult,
    errors: &mut Vec<ValidationError>,
) {
    let collection_value = serde_json::to_value(collection).expect("collection must serialize");
    match collector.status {
        CollectorStatus::Success | CollectorStatus::Partial => {
            for field in &collector.fields {
                match collection_value.pointer(&field.path) {
                    Some(Value::Null) => {}
                    Some(_) => push_error(
                        errors,
                        &field.path,
                        "field collection status must refer to a null value",
                    ),
                    None => push_error(
                        errors,
                        &field.path,
                        "field collection status refers to an unknown path",
                    ),
                }
            }
            let mut null_paths = Vec::new();
            collect_null_paths(&collection_value["cpu"], "/cpu", &mut null_paths);
            for path in null_paths {
                if !collector.fields.iter().any(|field| field.path == path) {
                    push_error(
                        errors,
                        path,
                        "null CPU value must have a field collection status",
                    );
                }
            }
            if collector.status == CollectorStatus::Success
                && (!collector.fields.is_empty() || !collector.messages.is_empty())
            {
                push_error(
                    errors,
                    "/collectors/cpu/status",
                    "successful CPU collector must not contain failures",
                );
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if CPU_FAILURE_PATHS.iter().any(|path| {
                collection_value
                    .pointer(path)
                    .is_some_and(|value| !value.is_null())
            }) {
                push_error(
                    errors,
                    "/cpu",
                    "skipped or failed CPU collector requires all values to be null",
                );
            }
            if collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/cpu/messages",
                    "skipped or failed CPU collector must include a reason",
                );
            }
        }
    }
}

fn validate_windows_status(
    collection: &Collection,
    collector: &CollectorResult,
    errors: &mut Vec<ValidationError>,
) {
    let value = serde_json::to_value(&collection.windows).expect("Windows model is serializable");
    match collector.status {
        CollectorStatus::Success | CollectorStatus::Partial => {
            for field in &collector.fields {
                validate_null_field_result(&value, field, "/windows", errors);
            }
            for path in WINDOWS_PATHS {
                let relative = path.strip_prefix("/windows").unwrap_or(path);
                if value.pointer(relative).is_some_and(Value::is_null)
                    && !collector.fields.iter().any(|field| field.path == path)
                {
                    push_error(
                        errors,
                        path,
                        "null value requires a field collection result",
                    );
                }
            }
            if collector.status == CollectorStatus::Success
                && (!collector.fields.is_empty() || !collector.messages.is_empty())
            {
                push_error(
                    errors,
                    "/collectors/windows/status",
                    "successful Windows collector must not contain failures",
                );
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if WINDOWS_PATHS.iter().any(|path| {
                let relative = path.strip_prefix("/windows").unwrap_or(path);
                value
                    .pointer(relative)
                    .is_some_and(|value| !value.is_null())
            }) {
                push_error(
                    errors,
                    "/windows",
                    "skipped or failed Windows collector requires all values to be null",
                );
            }
            if collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/windows/messages",
                    "skipped or failed Windows collector must include a reason",
                );
            }
        }
    }
}

fn validate_null_field_result(
    value: &Value,
    field: &crate::FieldCollectionResult,
    base: &str,
    errors: &mut Vec<ValidationError>,
) {
    let Some(relative) = field.path.strip_prefix(base) else {
        push_error(
            errors,
            &field.path,
            "field collection status refers to an unknown path",
        );
        return;
    };
    match value.pointer(relative) {
        Some(Value::Null) => {}
        Some(_) => push_error(
            errors,
            &field.path,
            "field collection status must refer to a null value",
        ),
        None => push_error(
            errors,
            &field.path,
            "field collection status refers to an unknown path",
        ),
    }
}

fn validate_single_storage_collector(
    collection: &Collection,
    status: &CollectionStatus,
    name: CollectorName,
    collection_path: &str,
    collection_is_some: bool,
    success_allows_not_applicable: bool,
    errors: &mut Vec<ValidationError>,
) {
    let collector_path = match name {
        CollectorName::Partitions => "/collectors/partitions",
        CollectorName::Volumes => "/collectors/volumes",
        CollectorName::Smart => "/collectors/smart",
        _ => "/collectors",
    };
    let collectors: Vec<_> = status
        .collectors
        .iter()
        .filter(|collector| collector.name == name)
        .collect();
    if collectors.len() > 1 {
        push_error(
            errors,
            "/collectors",
            format!("must not contain more than one {name:?} collector result"),
        );
        return;
    }
    let Some(collector) = collectors.first() else {
        return;
    };
    let collection_value = serde_json::to_value(collection).expect("collection must serialize");

    match collector.status {
        CollectorStatus::Success | CollectorStatus::Partial => {
            if !collection_is_some {
                push_error(
                    errors,
                    collection_path,
                    "successful or partial collector requires an array",
                );
                return;
            }
            for field in &collector.fields {
                match collection_value.pointer(&field.path) {
                    Some(Value::Null) => {}
                    Some(_) => push_error(
                        errors,
                        &field.path,
                        "field collection status must refer to a null value",
                    ),
                    None => push_error(
                        errors,
                        &field.path,
                        "field collection status refers to an unknown path",
                    ),
                }
            }
            if let Some(value) = collection_value.pointer(collection_path) {
                let mut null_paths = Vec::new();
                collect_null_paths(value, collection_path, &mut null_paths);
                for path in null_paths {
                    if !collector.fields.iter().any(|field| field.path == path) {
                        push_error(
                            errors,
                            path,
                            "null storage value must have a field collection status",
                        );
                    }
                }
            }
            if collector.status == CollectorStatus::Success {
                let invalid_field = if success_allows_not_applicable {
                    collector
                        .fields
                        .iter()
                        .any(|field| field.status != crate::FieldCollectionStatus::NotApplicable)
                } else {
                    !collector.fields.is_empty()
                };
                if invalid_field || !collector.messages.is_empty() {
                    push_error(
                        errors,
                        format!("{collector_path}/status"),
                        "success contains collection failures",
                    );
                }
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if collection_is_some {
                push_error(
                    errors,
                    collection_path,
                    "skipped or failed collector requires a null collection",
                );
            }
            if collector.messages.is_empty() {
                push_error(
                    errors,
                    format!("{collector_path}/messages"),
                    "skipped or failed collector must include a reason",
                );
            }
        }
    }
}

fn validate_physical_disk_status(
    collection: &Collection,
    collector: &CollectorResult,
    errors: &mut Vec<ValidationError>,
) {
    let collection_value = serde_json::to_value(collection).expect("collection must serialize");

    match collector.status {
        CollectorStatus::Success | CollectorStatus::Partial => {
            let Some(disks) = &collection.storage.disks else {
                push_error(
                    errors,
                    "/storage/disks",
                    "successful or partial physical disk collectors require a disk array",
                );
                return;
            };

            for field in &collector.fields {
                match collection_value.pointer(&field.path) {
                    Some(Value::Null) => {}
                    Some(_) => push_error(
                        errors,
                        &field.path,
                        "field collection status must refer to a null value",
                    ),
                    None => push_error(
                        errors,
                        &field.path,
                        "field collection status refers to an unknown path",
                    ),
                }
            }
            for path in physical_disk_null_paths(disks) {
                if !collector.fields.iter().any(|field| field.path == path) {
                    push_error(
                        errors,
                        path,
                        "null physical disk value must have a field collection status",
                    );
                }
            }
            if collector.status == CollectorStatus::Success
                && (!collector.fields.is_empty() || !collector.messages.is_empty())
            {
                push_error(
                    errors,
                    "/collectors/physical_disks/status",
                    "success cannot contain physical disk collection failures",
                );
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if collection.storage.disks.is_some() {
                push_error(
                    errors,
                    "/storage/disks",
                    "skipped or failed physical disk collectors require a null collection",
                );
            }
            if collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/physical_disks/messages",
                    "skipped or failed physical disk collectors must include a reason",
                );
            }
        }
    }
}

fn validate_device_status(
    collection: &Collection,
    collector: &CollectorResult,
    errors: &mut Vec<ValidationError>,
) {
    let collection_value = serde_json::to_value(collection).expect("collection must serialize");

    match collector.status {
        CollectorStatus::Success | CollectorStatus::Partial => {
            let Some(devices) = &collection.devices else {
                push_error(
                    errors,
                    "/devices",
                    "successful or partial device collectors require a device array",
                );
                return;
            };

            for field in &collector.fields {
                match collection_value.pointer(&field.path) {
                    Some(Value::Null) => {}
                    Some(_) => push_error(
                        errors,
                        &field.path,
                        "field collection status must refer to a null value",
                    ),
                    None => push_error(
                        errors,
                        &field.path,
                        "field collection status refers to an unknown path",
                    ),
                }
            }

            for path in device_null_paths(devices) {
                if !collector.fields.iter().any(|field| field.path == path) {
                    push_error(
                        errors,
                        path,
                        "null device value must have a field collection status",
                    );
                }
            }

            for (index, device) in devices.iter().enumerate() {
                if device.device_state.present == Some(false) {
                    for suffix in ["started", "problem_code"] {
                        let path = format!("/devices/{index}/device_state/{suffix}");
                        if collection_value.pointer(&path) == Some(&Value::Null)
                            && let Some(field) =
                                collector.fields.iter().find(|field| field.path == path)
                            && field.status != crate::FieldCollectionStatus::NotApplicable
                        {
                            push_error(
                                errors,
                                path,
                                "state unavailable because a device is absent must be not_applicable",
                            );
                        }
                    }
                }
            }

            if collector.status == CollectorStatus::Success
                && collector
                    .fields
                    .iter()
                    .any(|field| field.status != crate::FieldCollectionStatus::NotApplicable)
            {
                push_error(
                    errors,
                    "/collectors/devices/status",
                    "success may only contain not_applicable field statuses",
                );
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if collection.devices.is_some() {
                push_error(
                    errors,
                    "/devices",
                    "skipped or failed device collectors require a null device collection",
                );
            }
            if collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/devices/messages",
                    "skipped or failed device collectors must include a reason",
                );
            }
        }
    }
}

fn validate_gpu_status(
    collection: &Collection,
    collector: &CollectorResult,
    errors: &mut Vec<ValidationError>,
) {
    let collection_value = serde_json::to_value(collection).expect("collection must serialize");

    match collector.status {
        CollectorStatus::Success | CollectorStatus::Partial => {
            if collection.gpus.is_none() {
                push_error(
                    errors,
                    "/gpus",
                    "successful or partial GPU collectors require a GPU array",
                );
                return;
            }

            for field in &collector.fields {
                match collection_value.pointer(&field.path) {
                    Some(Value::Null) => {}
                    Some(_) => push_error(
                        errors,
                        &field.path,
                        "field collection status must refer to a null value",
                    ),
                    None => push_error(
                        errors,
                        &field.path,
                        "field collection status refers to an unknown path",
                    ),
                }
            }

            for path in gpu_null_paths(collection) {
                if !collector.fields.iter().any(|field| field.path == path) {
                    push_error(
                        errors,
                        path,
                        "null GPU value must have a field collection status",
                    );
                }
            }

            if collector.status == CollectorStatus::Success
                && collector
                    .fields
                    .iter()
                    .any(|field| field.status != crate::FieldCollectionStatus::NotApplicable)
            {
                push_error(
                    errors,
                    "/collectors/gpu/status",
                    "success may only contain not_applicable field statuses",
                );
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if collection.gpus.is_some() {
                push_error(
                    errors,
                    "/gpus",
                    "skipped or failed GPU collectors require a null GPU collection",
                );
            }
            if collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/gpu/messages",
                    "skipped or failed GPU collectors must include a reason",
                );
            }
        }
    }
}

fn gpu_null_paths(collection: &Collection) -> Vec<String> {
    let Some(gpus) = &collection.gpus else {
        return vec![];
    };
    let value = serde_json::to_value(gpus).expect("GPU collection must serialize");
    let mut paths = Vec::new();
    collect_null_paths(&value, "/gpus", &mut paths);
    paths
}

fn device_null_paths(devices: &[crate::ConnectedDevice]) -> Vec<String> {
    let value = serde_json::to_value(devices).expect("device collection must serialize");
    let mut paths = Vec::new();
    collect_null_paths(&value, "/devices", &mut paths);
    paths
}

fn physical_disk_null_paths(disks: &[crate::PhysicalDisk]) -> Vec<String> {
    let value = serde_json::to_value(disks).expect("physical disk collection must serialize");
    let mut paths = Vec::new();
    collect_null_paths(&value, "/storage/disks", &mut paths);
    paths
}

fn collect_null_paths(value: &Value, path: &str, paths: &mut Vec<String>) {
    match value {
        Value::Null => paths.push(path.into()),
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_null_paths(value, &format!("{path}/{index}"), paths);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                collect_null_paths(value, &format!("{path}/{key}"), paths);
            }
        }
        _ => {}
    }
}

impl Diagnosis {
    pub fn validate_against(&self, collection: &Collection) -> Result<(), ValidationErrors> {
        let mut errors = Vec::new();
        if self.rule_set.name.is_empty() {
            push_error(&mut errors, "/rule_set/name", "must not be empty");
        }
        if self.rule_set.version.is_empty() {
            push_error(&mut errors, "/rule_set/version", "must not be empty");
        }

        let mut rule_ids = HashSet::new();
        let mut passed = 0;
        let mut triggered = 0;
        let mut not_applicable = 0;
        let mut not_evaluated = 0;
        let mut failed = 0;
        let mut critical = 0;
        let mut error_findings = 0;
        let mut warning = 0;
        let mut information = 0;
        let mut highest_severity = None;

        for (evaluation_index, evaluation) in self.evaluations.iter().enumerate() {
            let base = format!("/evaluations/{evaluation_index}");

            if evaluation.rule_id.is_empty() || !rule_ids.insert(&evaluation.rule_id) {
                push_error(
                    &mut errors,
                    format!("{base}/rule_id"),
                    "must be non-empty and unique",
                );
            }
            match evaluation.status {
                RuleEvaluationStatus::Passed => passed += 1,
                RuleEvaluationStatus::Triggered => triggered += 1,
                RuleEvaluationStatus::NotApplicable => not_applicable += 1,
                RuleEvaluationStatus::NotEvaluated => not_evaluated += 1,
                RuleEvaluationStatus::Failed => failed += 1,
            }

            if evaluation.status == RuleEvaluationStatus::Triggered && evaluation.severity.is_none()
            {
                push_error(
                    &mut errors,
                    format!("{base}/severity"),
                    "triggered evaluations must have a severity",
                );
            }
            if evaluation.status == RuleEvaluationStatus::Triggered
                && let Some(severity) = evaluation.severity
            {
                match severity {
                    Severity::Critical => critical += 1,
                    Severity::Error => error_findings += 1,
                    Severity::Warning => warning += 1,
                    Severity::Information => information += 1,
                }
                if highest_severity
                    .map(|current| severity_rank(severity) > severity_rank(current))
                    .unwrap_or(true)
                {
                    highest_severity = Some(severity);
                }
            }

            if evaluation.status == RuleEvaluationStatus::NotEvaluated
                && evaluation.reason.is_none()
            {
                push_error(
                    &mut errors,
                    format!("{base}/reason"),
                    "not_evaluated evaluations must have a reason",
                );
            }

            for (evidence_index, evidence) in evaluation.evidence.iter().enumerate() {
                let evidence_path = format!("{base}/evidence/{evidence_index}");
                match evidence {
                    Evidence::Collected { path, value } => {
                        validate_collected_evidence(
                            collection,
                            path,
                            value,
                            &evidence_path,
                            &mut errors,
                        );
                    }
                    Evidence::Derived { source_paths, .. } => {
                        for source_path in source_paths {
                            match collection_value(collection, source_path) {
                                Some(Value::Null) | None => push_error(
                                    &mut errors,
                                    &evidence_path,
                                    format!(
                                        "derived evidence source does not resolve to a value: {source_path}"
                                    ),
                                ),
                                Some(_) => {}
                            }
                        }
                    }
                }
            }
        }

        let counts = &self.summary.evaluations;
        for (path, actual, expected) in [
            ("/summary/evaluations/passed", counts.passed, passed),
            (
                "/summary/evaluations/triggered",
                counts.triggered,
                triggered,
            ),
            (
                "/summary/evaluations/not_applicable",
                counts.not_applicable,
                not_applicable,
            ),
            (
                "/summary/evaluations/not_evaluated",
                counts.not_evaluated,
                not_evaluated,
            ),
            ("/summary/evaluations/failed", counts.failed, failed),
        ] {
            if actual != expected {
                push_error(&mut errors, path, "must match evaluations");
            }
        }
        let findings = &self.summary.findings;
        for (path, actual, expected) in [
            ("/summary/findings/critical", findings.critical, critical),
            ("/summary/findings/error", findings.error, error_findings),
            ("/summary/findings/warning", findings.warning, warning),
            (
                "/summary/findings/information",
                findings.information,
                information,
            ),
        ] {
            if actual != expected {
                push_error(&mut errors, path, "must match triggered evaluations");
            }
        }
        if self.summary.overall_severity != highest_severity {
            push_error(
                &mut errors,
                "/summary/overall_severity",
                "must be the highest triggered severity",
            );
        }

        finish(errors)
    }
}

fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Information => 1,
        Severity::Warning => 2,
        Severity::Error => 3,
        Severity::Critical => 4,
    }
}

fn validate_memory_status(
    collection: &Collection,
    collector: &CollectorResult,
    errors: &mut Vec<ValidationError>,
) {
    let null_paths: Vec<_> = MEMORY_PATHS
        .iter()
        .copied()
        .filter(|path| collection_value(collection, path) == Some(Value::Null))
        .collect();

    match collector.status {
        CollectorStatus::Success => {
            if !null_paths.is_empty() {
                push_error(
                    errors,
                    "/collectors/memory/status",
                    "success cannot contain null memory values",
                );
            }
            if !collector.fields.is_empty() {
                push_error(
                    errors,
                    "/collectors/memory/fields",
                    "success cannot contain field failures",
                );
            }
        }
        CollectorStatus::Partial => {
            for path in &null_paths {
                if !collector.fields.iter().any(|field| field.path == *path) {
                    push_error(
                        errors,
                        *path,
                        "null value must have a field collection result",
                    );
                }
            }

            for field in &collector.fields {
                match collection_value(collection, &field.path) {
                    Some(Value::Null) => {}
                    Some(_) => push_error(
                        errors,
                        &field.path,
                        "field collection failure must refer to a null value",
                    ),
                    None => push_error(
                        errors,
                        &field.path,
                        "field collection failure refers to an unknown path",
                    ),
                }
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/memory/messages",
                    "skipped or failed collectors must include a reason",
                );
            }
            if null_paths.len() != MEMORY_PATHS.len() {
                push_error(
                    errors,
                    "/memory",
                    "skipped or failed memory collectors require all memory values to be null",
                );
            }
        }
    }
}

fn validate_collected_evidence(
    collection: &Collection,
    path: &str,
    expected: &Value,
    evidence_path: &str,
    errors: &mut Vec<ValidationError>,
) {
    match collection_value(collection, path) {
        Some(Value::Null) => push_error(
            errors,
            evidence_path,
            format!("collected evidence refers to a null value: {path}"),
        ),
        Some(actual) if &actual != expected => push_error(
            errors,
            evidence_path,
            format!("collected evidence value does not match {path}"),
        ),
        Some(_) => {}
        None => push_error(
            errors,
            evidence_path,
            format!("collected evidence refers to an unknown path: {path}"),
        ),
    }
}

fn collection_value(collection: &Collection, path: &str) -> Option<Value> {
    serde_json::to_value(collection)
        .expect("collection must serialize")
        .pointer(path)
        .cloned()
}

fn validate_available_not_greater_than_total(
    errors: &mut Vec<ValidationError>,
    base_path: &str,
    available: Option<u64>,
    total: Option<u64>,
) {
    if let (Some(available), Some(total)) = (available, total)
        && available > total
    {
        push_error(
            errors,
            format!("{base_path}/available_bytes"),
            "must not be greater than the corresponding total or limit",
        );
    }
}

fn validate_storage_range(
    errors: &mut Vec<ValidationError>,
    base: &str,
    disk_number: u32,
    offset: u64,
    length: u64,
    disks: Option<&[crate::PhysicalDisk]>,
) {
    let Some(disks) = disks else {
        return;
    };
    let Some(disk) = disks.iter().find(|disk| disk.number == disk_number) else {
        push_error(
            errors,
            format!("{base}/disk_number"),
            "must refer to a collected physical disk",
        );
        return;
    };
    if let Some(capacity) = disk.capacity_bytes
        && offset.checked_add(length).is_none_or(|end| end > capacity)
    {
        push_error(
            errors,
            format!("{base}/length_bytes"),
            "range must fit within the referenced physical disk",
        );
    }
}

fn is_mbr_extended_partition(partition: &crate::DiskPartition) -> bool {
    partition.style == crate::PartitionStyle::Mbr
        && partition.type_id.as_deref().is_some_and(|type_id| {
            matches!(
                type_id.to_ascii_uppercase().as_str(),
                "0X05" | "0X0F" | "0X85"
            )
        })
}

fn push_error(
    errors: &mut Vec<ValidationError>,
    path: impl Into<String>,
    message: impl Into<String>,
) {
    errors.push(ValidationError {
        path: path.into(),
        message: message.into(),
    });
}

fn finish(errors: Vec<ValidationError>) -> Result<(), ValidationErrors> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationErrors(errors))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        ClockCollection, CollectionMessage, CollectorStatus, CommitMemory, CpuCollection,
        CpuFeatures, CpuInstructionSet, CpuPackage, CpuTopology, Criterion, DiagnosisSummary,
        DiskBusType, DiskPartition, EvaluationCounts, FieldCollectionResult, FieldCollectionStatus,
        FindingCounts, FirmwareCollection, FirmwareInterfaceType, MeasurementUnit,
        MemoryCollection, PartitionStyle, PhysicalDisk, PhysicalMemory, Recommendation,
        RuleEvaluation, RuleSetInfo, Severity, StorageCollection, VirtualMemory, WindowsCollection,
    };

    #[test]
    fn collection_round_trips_with_virtual_json_name() {
        let collection = complete_collection();
        let json = serde_json::to_value(&collection).unwrap();

        assert!(json["memory"].get("virtual").is_some());
        assert!(json["memory"].get("virtual_memory").is_none());
        assert_eq!(
            serde_json::from_value::<Collection>(json).unwrap(),
            collection
        );
    }

    #[test]
    fn rejects_overlapping_partitions_on_the_same_disk() {
        let mut collection = complete_collection();
        collection.storage.partitions = Some(vec![
            DiskPartition {
                disk_number: 0,
                partition_number: 1,
                offset_bytes: 1_048_576,
                length_bytes: 10_000,
                style: PartitionStyle::Gpt,
                type_id: None,
                bootable: None,
            },
            DiskPartition {
                disk_number: 0,
                partition_number: 2,
                offset_bytes: 1_050_000,
                length_bytes: 10_000,
                style: PartitionStyle::Gpt,
                type_id: None,
                bootable: None,
            },
        ]);

        let errors = collection.validate().unwrap_err();
        assert!(errors.errors().iter().any(|error| {
            error.path == "/storage/partitions/1/offset_bytes"
                && error.message.contains("must not overlap")
        }));
    }

    #[test]
    fn allows_logical_partition_inside_mbr_extended_container() {
        let mut collection = complete_collection();
        collection.storage.disks = Some(vec![PhysicalDisk {
            number: 0,
            model: Some("Example Disk".into()),
            manufacturer: None,
            firmware_revision: None,
            bus_type: Some(DiskBusType::Sata),
            capacity_bytes: Some(1_000_000),
            logical_sector_size_bytes: Some(512),
            removable: Some(false),
        }]);
        collection.storage.partitions = Some(vec![
            DiskPartition {
                disk_number: 0,
                partition_number: 1,
                offset_bytes: 100_000,
                length_bytes: 800_000,
                style: PartitionStyle::Mbr,
                type_id: Some("0x0F".into()),
                bootable: Some(false),
            },
            DiskPartition {
                disk_number: 0,
                partition_number: 2,
                offset_bytes: 200_000,
                length_bytes: 100_000,
                style: PartitionStyle::Mbr,
                type_id: Some("0x07".into()),
                bootable: Some(false),
            },
        ]);

        collection.validate().unwrap();
    }

    #[test]
    fn partial_collection_requires_reasons_for_null_fields() {
        let mut collection = complete_collection();
        collection.memory.commit.limit_bytes = None;

        let status = CollectionStatus {
            collectors: vec![CollectorResult {
                name: CollectorName::Memory,
                status: CollectorStatus::Partial,
                duration_ms: 12,
                messages: vec![],
                fields: vec![],
            }],
        };

        let errors = collection.validate_with_status(&status).unwrap_err();
        assert!(errors.errors().iter().any(|error| {
            error.path == "/memory/commit/limit_bytes"
                && error
                    .message
                    .contains("must have a field collection result")
        }));
    }

    #[test]
    fn partial_collection_accepts_an_explained_null() {
        let mut collection = complete_collection();
        collection.memory.commit.limit_bytes = None;

        let status = CollectionStatus {
            collectors: vec![CollectorResult {
                name: CollectorName::Memory,
                status: CollectorStatus::Partial,
                duration_ms: 12,
                messages: vec![],
                fields: vec![FieldCollectionResult {
                    path: "/memory/commit/limit_bytes".into(),
                    status: FieldCollectionStatus::SourceNull,
                    code: "source_returned_null".into(),
                    native_code: None,
                }],
            }],
        };

        collection.validate_with_status(&status).unwrap();
    }

    #[test]
    fn failed_collector_explains_all_null_memory_values() {
        let collection = null_collection();
        let status = CollectionStatus {
            collectors: vec![CollectorResult {
                name: CollectorName::Memory,
                status: CollectorStatus::Failed,
                duration_ms: 3,
                messages: vec![CollectionMessage {
                    code: "windows_api_failed".into(),
                    native_code: Some(5),
                    message: Some("メモリ情報を取得できませんでした".into()),
                }],
                fields: vec![],
            }],
        };

        collection.validate_with_status(&status).unwrap();
    }

    #[test]
    fn diagnosis_evidence_must_match_the_collection() {
        let collection = complete_collection();
        let mut diagnosis = memory_diagnosis();
        diagnosis.evaluations[0].evidence[0] = Evidence::Collected {
            path: "/memory/physical/total_bytes".into(),
            value: json!(1),
        };

        let errors = diagnosis.validate_against(&collection).unwrap_err();
        assert!(errors.errors().iter().any(|error| {
            error
                .message
                .contains("does not match /memory/physical/total_bytes")
        }));
    }

    #[test]
    fn valid_memory_diagnosis_matches_the_collection() {
        memory_diagnosis()
            .validate_against(&complete_collection())
            .unwrap();
    }

    fn complete_collection() -> Collection {
        Collection {
            windows: windows_collection(),
            windows_updates: Default::default(),
            clock: clock_collection(),
            cpu: cpu_collection(),
            firmware: firmware_collection(),
            memory: MemoryCollection {
                physical: PhysicalMemory {
                    total_bytes: Some(17_179_869_184),
                    available_bytes: Some(536_870_912),
                    load_percent: Some(97.0),
                },
                commit: CommitMemory {
                    limit_bytes: Some(25_769_803_776),
                    available_bytes: Some(9_126_805_504),
                },
                virtual_memory: VirtualMemory {
                    total_bytes: Some(140_737_488_224_256),
                    available_bytes: Some(140_732_881_338_368),
                },
            },
            gpus: Some(vec![]),
            devices: Some(vec![]),
            event_logs: Default::default(),
            storage: StorageCollection {
                disks: Some(vec![]),
                partitions: Some(vec![]),
                volumes: Some(vec![]),
                smart: Some(vec![]),
            },
        }
    }

    fn null_collection() -> Collection {
        Collection {
            windows: windows_collection(),
            windows_updates: Default::default(),
            clock: clock_collection(),
            cpu: cpu_collection(),
            firmware: firmware_collection(),
            memory: MemoryCollection {
                physical: PhysicalMemory {
                    total_bytes: None,
                    available_bytes: None,
                    load_percent: None,
                },
                commit: CommitMemory {
                    limit_bytes: None,
                    available_bytes: None,
                },
                virtual_memory: VirtualMemory {
                    total_bytes: None,
                    available_bytes: None,
                },
            },
            gpus: Some(vec![]),
            devices: Some(vec![]),
            event_logs: Default::default(),
            storage: StorageCollection {
                disks: Some(vec![]),
                partitions: Some(vec![]),
                volumes: Some(vec![]),
                smart: Some(vec![]),
            },
        }
    }

    fn windows_collection() -> WindowsCollection {
        WindowsCollection {
            edition: Some("Professional".into()),
            version: Some("10.0.26100".into()),
            build_number: Some(26_100),
            architecture: Some(crate::SystemArchitecture::X86_64),
            booted_at: Some("2026-07-17T00:00:00.000Z".into()),
            uptime_ms: Some(123_000),
            boot_mode: Some(crate::BootMode::Uefi),
        }
    }

    fn clock_collection() -> ClockCollection {
        ClockCollection {
            system_time_utc: Some("2026-07-17T00:02:03.000Z".into()),
            utc_offset_minutes: Some(540),
            windows_time_service: Some(crate::WindowsServiceState::Running),
            hardware_clock: None,
        }
    }

    fn cpu_collection() -> CpuCollection {
        CpuCollection {
            architecture: Some(crate::SystemArchitecture::X86_64),
            topology: CpuTopology {
                physical_packages: Some(1),
                physical_cores: Some(14),
                logical_processors: Some(20),
            },
            packages: Some(vec![CpuPackage {
                package_index: 0,
                manufacturer: Some("GenuineIntel".into()),
                model: Some("Example CPU".into()),
                physical_cores: Some(14),
                logical_processors: Some(20),
            }]),
            features: CpuFeatures {
                available_instruction_sets: Some(vec![
                    CpuInstructionSet::Sse2,
                    CpuInstructionSet::Avx,
                    CpuInstructionSet::Avx2,
                ]),
                hardware_virtualization_extensions_available: Some(true),
                virtualization_firmware_enabled: Some(true),
                hypervisor_present: Some(false),
            },
        }
    }

    fn firmware_collection() -> FirmwareCollection {
        FirmwareCollection {
            vendor: Some("Example Vendor".into()),
            version: Some("1.2.3".into()),
            release_date: Some("2026-07-17".into()),
            interface_type: Some(FirmwareInterfaceType::Uefi),
            secure_boot_enabled: Some(true),
            status: None,
        }
    }

    fn memory_diagnosis() -> Diagnosis {
        Diagnosis {
            rule_set: RuleSetInfo {
                name: "pcdiag_builtin".into(),
                version: "0.1.0".into(),
            },
            summary: DiagnosisSummary {
                overall_severity: Some(Severity::Warning),
                evaluations: EvaluationCounts {
                    passed: 0,
                    triggered: 1,
                    not_applicable: 0,
                    not_evaluated: 0,
                    failed: 0,
                },
                findings: FindingCounts {
                    critical: 0,
                    error: 0,
                    warning: 1,
                    information: 0,
                },
            },
            evaluations: vec![RuleEvaluation {
                rule_id: "memory.available_ratio".into(),
                rule_version: "1.0".into(),
                category: "memory".into(),
                status: RuleEvaluationStatus::Triggered,
                severity: Some(Severity::Warning),
                summary: "使用可能な物理メモリが少なくなっています".into(),
                evidence: vec![
                    Evidence::Collected {
                        path: "/memory/physical/total_bytes".into(),
                        value: json!(17_179_869_184_u64),
                    },
                    Evidence::Collected {
                        path: "/memory/physical/available_bytes".into(),
                        value: json!(536_870_912_u64),
                    },
                    Evidence::Derived {
                        name: "available_percent".into(),
                        value: json!(3.125),
                        unit: Some(MeasurementUnit::Percent),
                        source_paths: vec![
                            "/memory/physical/total_bytes".into(),
                            "/memory/physical/available_bytes".into(),
                        ],
                    },
                ],
                criterion: Some(Criterion {
                    operator: "less_than".into(),
                    threshold: json!(10.0),
                    unit: Some(MeasurementUnit::Percent),
                }),
                reason: None,
                recommendation: Some(Recommendation {
                    code: "review_memory_consumption".into(),
                }),
            }],
        }
    }
}
