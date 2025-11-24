// Copyright (c) Kyutai, all rights reserved.
// This source code is licensed under the license found in the
// LICENSE file in the root directory of this source tree.

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};

/// List all available input and output devices
pub fn list_devices() -> Result<()> {
    let host = cpal::default_host();
    
    println!("\n=== Input Devices ===");
    let input_devices: Vec<_> = host.input_devices()?.collect();
    if input_devices.is_empty() {
        println!("  (none)");
    } else {
        for (idx, device) in input_devices.iter().enumerate() {
            let name = device.name().unwrap_or_else(|_| "(unknown)".to_string());
            println!("  {}. {}", idx + 1, name);
        }
    }
    
    println!("\n=== Output Devices ===");
    let output_devices: Vec<_> = host.output_devices()?.collect();
    if output_devices.is_empty() {
        println!("  (none)");
    } else {
        for (idx, device) in output_devices.iter().enumerate() {
            let name = device.name().unwrap_or_else(|_| "(unknown)".to_string());
            println!("  {}. {}", idx + 1, name);
        }
    }
    
    Ok(())
}

/// Find input device by case-insensitive substring match
pub fn find_input_device(query: &str) -> Result<cpal::Device> {
    let host = cpal::default_host();
    let query_lower = query.to_lowercase();
    
    let mut matches = vec![];
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            if name.to_lowercase().contains(&query_lower) {
                matches.push((device, name));
            }
        }
    }
    
    match matches.len() {
        0 => {
            eprintln!("No input device found matching '{}'", query);
            eprintln!("\nAvailable input devices:");
            for device in host.input_devices()? {
                if let Ok(name) = device.name() {
                    eprintln!("  - {}", name);
                }
            }
            bail!("No matching input device found");
        }
        1 => {
            let (device, name) = matches.into_iter().next().unwrap();
            tracing::info!("Selected input device: {}", name);
            Ok(device)
        }
        _ => {
            let (device, name) = matches[0].clone();
            tracing::warn!(
                "Multiple devices matched '{}', using first: {}",
                query,
                name
            );
            for (_, other_name) in matches.iter().skip(1) {
                tracing::warn!("  Also matched: {}", other_name);
            }
            Ok(device)
        }
    }
}

/// Find output device by case-insensitive substring match, fallback to default
pub fn find_output_device(query: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    
    let query = match query {
        None => {
            let device = host.default_output_device()
                .context("No default output device available")?;
            let name = device.name().unwrap_or_else(|_| "(unknown)".to_string());
            tracing::info!("Using default output device: {}", name);
            return Ok(device);
        }
        Some(q) => q,
    };
    
    let query_lower = query.to_lowercase();
    let mut matches = vec![];
    for device in host.output_devices()? {
        if let Ok(name) = device.name() {
            if name.to_lowercase().contains(&query_lower) {
                matches.push((device, name));
            }
        }
    }
    
    match matches.len() {
        0 => {
            tracing::warn!("No output device found matching '{}', using default", query);
            let device = host.default_output_device()
                .context("No default output device available")?;
            let name = device.name().unwrap_or_else(|_| "(unknown)".to_string());
            tracing::info!("Using default output device: {}", name);
            Ok(device)
        }
        1 => {
            let (device, name) = matches.into_iter().next().unwrap();
            tracing::info!("Selected output device: {}", name);
            Ok(device)
        }
        _ => {
            let (device, name) = matches[0].clone();
            tracing::warn!(
                "Multiple devices matched '{}', using first: {}",
                query,
                name
            );
            for (_, other_name) in matches.iter().skip(1) {
                tracing::warn!("  Also matched: {}", other_name);
            }
            Ok(device)
        }
    }
}
