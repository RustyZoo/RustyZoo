//! What a run tells about itself in text, for the `mcp` server.

use super::drive::Run;
use crate::unit::Part;

impl Run {
    /// The date, the cash, the zoo rating and the counts of units and exhibits, one `name value`
    /// per line.
    pub fn summary(&self) -> String {
        let d = self.sim.calendar.date();
        let (mut animals, mut guests, mut staff) = (0, 0, 0);
        for unit in self.world.units.iter() {
            match unit.part {
                Part::Animal(_) => animals += 1,
                Part::Guest(_) => guests += 1,
                Part::Staff(_) => staff += 1,
            }
        }
        format!(
            "date {:04}-{:02}-{:02} {:02}:{:02}\ncash {}\nrating {}\nanimals {animals}\n\
             guests {guests}\nstaff {staff}\nexhibits {}\nentities {}\nawards {}",
            d.year,
            d.month,
            d.day,
            d.hour,
            d.minute,
            self.money.cash,
            self.sim.status.rating,
            self.world.exhibits().list().len(),
            self.world.zoo.entities.len(),
            self.world.zoo.awards.len(),
        )
    }

    /// The fields of the entity with this id (its record, and the live state of a unit); none
    /// when there is no such entity.
    pub fn describe(&self, id: u32) -> Option<String> {
        let entity = &self.world.zoo.entities[self.world.find(id)?];
        let mut text = format!("{entity:#?}");
        if let Some(unit) = self.world.units.get(id) {
            text += &format!("\n{unit:#?}");
        }
        Some(text)
    }
}
