// Copyright (C) 2026 YuzakiKokuban <heibanbaize@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{env, process::exit};

use anyhow::Result;
use notify::{NotifyRequest, send_output_dir_notification};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let topic_id = if args.len() > 1 {
        Some(args[1].parse::<i64>()?)
    } else {
        None
    };
    let event_label = if args.len() > 2 {
        &args[2]
    } else {
        "New Yield (新产物)"
    };

    match send_output_dir_notification(
        &NotifyRequest::new("output", event_label).with_topic_id(topic_id),
    ) {
        Ok(()) => Ok(()),
        Err(error) => {
            eprintln!("Error: {error}");
            exit(1);
        }
    }
}
