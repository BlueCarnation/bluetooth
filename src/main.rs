use btleplug::api::AddressType;
use btleplug::api::{Central, Manager as ApiManager, Peripheral};
use btleplug::platform::Manager;
use btleplug::Result;
use serde_json::{json, to_writer_pretty, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, Read};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    run_bluetooth_script().await?;
    Ok(())
}

pub async fn run_bluetooth_script() -> Result<bool> {
    // Open the file in read-only mode with buffer.
    let mut file = File::open("config.json").expect("Cannot open file");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Cannot read file");

    // Parse the string of data into serde_json::Value.
    let v: Value = serde_json::from_str(&contents).expect("Cannot parse JSON");

    let manager = Manager::new().await?;

    let adapters = manager.adapters().await?;
    let mut device_data = HashMap::new();

    if let Some(adapter) = adapters.into_iter().nth(0) {
        // Check the value of "instant_scan".
        match v.get("instant_scan") {
            Some(instant_scan) => {
                if instant_scan == &Value::Bool(true) {
                    println!("\nScan was set to be instant, starting scan...");
                    adapter.start_scan(Default::default()).await?;

                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    adapter.stop_scan().await?;

                    let devices = adapter.peripherals().await?;

                    for (index, device) in devices.iter().enumerate() {
                        if let Some(properties) = device.properties().await? {
                            let raw_manufacturer = get_manufacturer(&properties.address.to_string()).await.unwrap_or_else(|| "Unknown".to_string());
                            let manufacturer = sanitize_string(raw_manufacturer);
                            let sanitized_local_name = sanitize_string(properties.local_name.unwrap_or_else(|| "Unknown".to_string()));


                            // Serialize manufacturer_data as a JSON string
                            let manufacturer_data_json =
                                serde_json::to_string(&properties.manufacturer_data)
                                    .unwrap_or("{}".to_string());
                            let service_data_json = serde_json::to_string(&properties.service_data)
                                .unwrap_or("{}".to_string());
                            let services_json = if !properties.services.is_empty() {
                                let service_strings: Vec<String> =
                                    properties.services.iter().map(|s| s.to_string()).collect();
                                format!("[{}]", service_strings.join(", "))
                            } else {
                                "[]".to_string()
                            };

                            let device_info = json!({
                                "address_type": address_type_to_string(properties.address_type),
                                "classe": class_to_string(properties.class),
                                "fabricant": manufacturer,
                                "local_name": sanitized_local_name,
                                "mac_bluetooth": properties.address,
                                "manufacturer_data": manufacturer_data_json,
                                "rssi": rssi_to_string(properties.rssi),
                                "service_data": service_data_json, // Use the serialized string
                                "services": services_json, // Use the serialized string
                                "tx_power_level": tx_power_level_to_string(properties.tx_power_level),
                            });

                            device_data.insert(index.to_string(), device_info);
                        }
                    }

                    if let Ok(file) = File::create("bluetooth_instantdata.json") {
                        to_writer_pretty(file, &device_data)
                            .expect("Erreur lors de l'écriture dans le fichier JSON");
                        println!("{}", serde_json::to_string_pretty(&device_data).unwrap());
                    } else {
                        println!("Erreur lors de la création du fichier 'bluetooth_data.json'");
                    }
                } else {
                    println!("\nScan was set to be delayed");
                
                    let start_after_duration = v
                        .get("start_after_duration")
                        .unwrap_or(&Value::Number(serde_json::Number::from(0)))
                        .as_u64()
                        .unwrap();
                    let scan_duration = v
                        .get("scan_duration")
                        .unwrap_or(&Value::Number(serde_json::Number::from(0)))
                        .as_u64()
                        .unwrap();
                
                    for i in (1..=start_after_duration).rev() {
                        println!("Scan starts in {} seconds", i);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                
                    println!("Scan started, it will last for {} seconds...", scan_duration);
                    adapter.start_scan(Default::default()).await?;
                    let scan_start_time = Instant::now();
                
                    let mut device_intervals: HashMap<String, Vec<(u64, u64)>> = HashMap::new();
                    let mut device_details: HashMap<String, serde_json::Value> = HashMap::new();
                
                    while Instant::now() - scan_start_time < tokio::time::Duration::from_secs(scan_duration) {
                        let devices = adapter.peripherals().await?;
                        let now = Instant::now().duration_since(scan_start_time).as_secs();
                
                        for device in devices.iter() {
                            if let Some(properties) = device.properties().await? {
                                let device_id = properties.address.to_string();
                
                                let entry = device_intervals.entry(device_id.clone()).or_insert_with(Vec::new);
                                if let Some(last) = entry.last_mut() {
                                    // Extend the current interval if within 5 seconds of the last detection
                                    if now - last.1 <= 5 {
                                        last.1 = now;
                                    } else {
                                        // Start a new interval after a 5-second gap
                                        entry.push((now, now));
                                    }
                                } else {
                                    // Start the first interval for a new device
                                    entry.push((now, now));
                                }
                
                                // Update or insert detailed device information
                                let raw_manufacturer = get_manufacturer(&properties.address.to_string()).await.unwrap_or_else(|| "Unknown".to_string());
                                let manufacturer = sanitize_string(raw_manufacturer);
                                let sanitized_local_name = sanitize_string(properties.local_name.unwrap_or_else(|| "Unknown".to_string()));
                                let manufacturer_data_json = serde_json::to_string(&properties.manufacturer_data).unwrap_or("{}".to_string());
                                let service_data_json = serde_json::to_string(&properties.service_data).unwrap_or("{}".to_string());
                                let services_json = if !properties.services.is_empty() {
                                    let service_strings: Vec<String> = properties.services.iter().map(|s| s.to_string()).collect();
                                    format!("[{}]", service_strings.join(", "))
                                } else {
                                    "[]".to_string()
                                };
                
                                device_details.insert(device_id.clone(), json!({
                                    "address_type": address_type_to_string(properties.address_type),
                                    "classe": class_to_string(properties.class),
                                    "fabricant": manufacturer,
                                    "local_name": sanitized_local_name,
                                    "mac_bluetooth": properties.address,
                                    "manufacturer_data": manufacturer_data_json,
                                    "rssi": rssi_to_string(properties.rssi),
                                    "service_data": service_data_json,
                                    "services": services_json,
                                    "tx_power_level": tx_power_level_to_string(properties.tx_power_level),
                                }));
                            }
                        }
                
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                
                    adapter.stop_scan().await?;
                
                    // After scanning, update device_details with intervals information
                    for (device_id, intervals) in device_intervals.iter() {
                        let intervals_str = intervals.iter()
                            .map(|(start, end)| format!("{}-{}", start, end))
                            .collect::<Vec<_>>().join(",");
                        if let Some(device) = device_details.get_mut(device_id) {
                            device["bluetooth_durations"] = json!(intervals_str);
                        }
                    }
                
                    if let Ok(file) = File::create("bluetooth_scheduleddata.json") {
                        to_writer_pretty(file, &device_details).expect("Error writing to the JSON file");
                        println!("{}", serde_json::to_string_pretty(&device_details).unwrap());
                    } else {
                        println!("Error creating 'bluetooth_scheduleddata.json' file");
                    }
                }
            }
            None => println!("instant_scan does not exist"),
        }
    } else {
        println!("Aucun adaptateur Bluetooth trouvé.");
    }
    Ok(!device_data.is_empty())
}

fn sanitize_string(input: String) -> String {
    input.replace("'", " ").replace("`", " ").replace("\"", " ")
}

fn address_type_to_string(address_type: Option<AddressType>) -> String {
    address_type.map_or("Unknown".to_string(), |at| format!("{:?}", at))
}

fn class_to_string(class: Option<u32>) -> String {
    class.map_or("Unknown".to_string(), |c| c.to_string())
}

fn tx_power_level_to_string(tx_power_level: Option<i16>) -> String {
    tx_power_level.map_or("Unknown".to_string(), |tpl| tpl.to_string())
}

fn rssi_to_string(rssi: Option<i16>) -> String {
    rssi.map_or("Unknown".to_string(), |r| r.to_string())
}

async fn get_manufacturer(address: &str) -> Option<String> {
    let prefix_to_search: String = address.split(':').take(3).collect();
    if let Ok(file) = File::open("src/database/oui.csv") {
        let reader = io::BufReader::new(file);
        for line in reader.lines() {
            if let Ok(line) = line {
                let cols: Vec<&str> = line.split(',').collect();
                if cols.len() >= 3 && prefix_to_search == cols[1] {
                    return Some(cols[2].to_string());
                }
            }
        }
    }
    Some("Unknown".to_string())
}
