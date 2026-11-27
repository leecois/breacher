//! One call for front ends: frame in, serialisable answer out.
use crate::glyph::Templates;
use crate::image::Frame;
use crate::plan::{plan, Plan, PlanOptions};
use crate::reader::{read, ReadError, ReadOptions, Reading, Stage};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum Response {
    Ok {
        reading: Reading,
        plan: Plan,
    },
    Rejected {
        stage: Stage,
        message: String,
        /// What the player can do about it.
        hint: String,
    },
}

/// Short, actionable explanation of a rejection.
pub fn hint(stage: Stage) -> &'static str {
    match stage {
        Stage::Frame => "The image is empty or too small.",
        Stage::Segment | Stage::Lattice => "No Breach Protocol screen in this frame. Open the breach and capture before the first pick.",
        Stage::InProgress => "The breach has already started (some codes are used). Capture before the first pick.",
        Stage::Sequences => "Found the code matrix but could not read the sequence list. Make sure the whole breach screen is visible.",
        Stage::Buffer => "Found the matrix and sequences but could not size the buffer. Make sure the buffer panel is visible, or set the buffer size by hand.",
        Stage::Confidence => "A code is covered, already used, or too soft to read safely. Capture before the first pick, with the cursor off the matrix.",
    }
}

pub fn rejected(e: ReadError) -> Response {
    Response::Rejected { stage: e.stage, hint: hint(e.stage).to_string(), message: e.message }
}

/// Whether a breach screen is on this frame in any state: a code matrix was found, even if the rest did not read.
pub fn breach_on_screen(frame: &Frame) -> bool {
    match crate::reader::locate(frame, &ReadOptions::default()) {
        Ok(_) => true,
        Err(e) => e.stage >= Stage::InProgress,
    }
}

pub fn read_and_plan(
    frame: &Frame,
    templates: &Templates,
    read_opt: &ReadOptions,
    plan_opt: &PlanOptions,
) -> Response {
    match read(frame, templates, read_opt) {
        Ok(reading) => {
            let plan = plan(&reading, plan_opt);
            Response::Ok { reading, plan }
        }
        Err(e) => rejected(e),
    }
}
