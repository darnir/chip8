use rand::Rng;
use std::fmt;
use std::fs::File;
use std::io::Read;

use crate::instructions::{Instruction, InstructionParser};

const MEMORY_SIZE: usize = 4096;
const STACK_SIZE: usize = 16;
const REGISTER_COUNT: usize = 16;
const PROGRAM_OFFSET: usize = 512;
const FLAG_REGISTER: usize = 15;

struct Memory {
    mem: [u8; MEMORY_SIZE],
}

impl fmt::Debug for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const ZERO: u8 = 0;
        write!(f, "{{ ")?;
        for (index, byte) in self.mem.iter().enumerate() {
            if *byte != ZERO {
                write!(f, "{}: {}, ", index, byte)?;
            }
        }
        write!(f, "}}")
    }
}

pub struct Machine<T: InstructionParser> {
    name: String,
    counter: u16,
    stack_ptr: u8,
    mem: Memory,
    stack: [u16; STACK_SIZE],
    v: [u8; REGISTER_COUNT], // registers: v0 to vf
    i: u16,                  // "There is also a 16-bit register called I."
    delay_register: u8,
    sound_register: u8,
    instruction_parser: T,
    skip_increment: bool,
}

impl<T> fmt::Debug for Machine<T>
where
    T: InstructionParser,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {{ \n\tPC: {}, \n\tSP: {}, \n\tStack: {:?}, \n\tRegisters: {:?}, \n\ti: {}, \n\tDR: {}, \n\tSR: {}, \n\tSKIP: {} }}", self.name, self.counter, self.stack_ptr, self.stack, self.v, self.i, self.delay_register, self.sound_register, self.skip_increment)
    }
}

impl<T> Machine<T>
where
    T: InstructionParser,
{
    pub fn new(name: &str, ins_parser: T) -> Self {
        Self {
            name: name.to_string(),
            counter: 512,
            stack_ptr: 0,
            mem: Memory {
                mem: [0; MEMORY_SIZE],
            },
            stack: [0; STACK_SIZE],
            v: [0; REGISTER_COUNT],
            i: 0,
            delay_register: 0,
            sound_register: 0,
            instruction_parser: ins_parser,
            skip_increment: false,
        }
    }

    pub fn load_rom(&mut self, filename: &str) -> Result<(), std::io::Error> {
        let mut file = File::open(filename)?;
        self._copy_into_mem(&mut file)?;
        debug!("{:?}", self.mem);
        Ok(())
    }

    fn _copy_into_mem(&mut self, file: &mut File) -> Result<(), std::io::Error> {
        const BUFSIZE: usize = MEMORY_SIZE - PROGRAM_OFFSET;
        let mut buffer: [u8; BUFSIZE] = [0; BUFSIZE];

        // load the ROM into the buffer
        let _ = file.read(&mut buffer)?;

        // Copy the buffer into the VM memory
        // TODO: Why not copy directly without the intermediate buffer
        self.mem.mem[PROGRAM_OFFSET..].clone_from_slice(&buffer);
        Ok(())
    }

    /**
    * Create a 16-bit opcode out of 2 bytes
    * Ref: <https://stackoverflow.com/a/50244328>
    * Shift the bits by 8 to the left:
        (XXXXXXXX becomes XXXXXXXX00000000)
    * THEN bitwise-OR to concatenate them:
    *   (XXXXXXXX00000000 | YYYYYYYY) = XXXXXXXXYYYYYYYY
    **/
    fn get_opcode(b: &[u8]) -> u16 {
        let mut fb = u16::from(b[0]);
        let sb = u16::from(b[1]);
        fb <<= 8;
        fb | sb
    }

    fn inc_pc(&mut self) {
        self.counter += 2;
    }

    #[allow(clippy::cast_possible_truncation)]
    fn add(&mut self, d1: u8, d2: u8) -> u8 {
        let res: u16 = u16::from(d1) + u16::from(d2);
        self.v[FLAG_REGISTER] = if res > u16::from(u8::max_value()) {
            1
        } else {
            0
        };
        res as u8
    }

    #[allow(clippy::cast_possible_truncation)]
    fn add_16(&mut self, d1: u16, d2: u16) -> u16 {
        let res: u32 = u32::from(d1) + u32::from(d2);
        self.v[FLAG_REGISTER] = if res > u32::from(u16::max_value()) {
            1
        } else {
            0
        };
        res as u16
    }

    fn execute(&mut self, ins: &Instruction) {
        match *ins {
            Instruction::ClearScreen => {}
            Instruction::Return => {
                self.counter = self.stack[usize::from(self.stack_ptr)];
                self.stack_ptr -= 1;
                self.skip_increment = true;
            }
            Instruction::SYS => {}
            Instruction::Jump(address) => {
                self.counter = address;
                self.skip_increment = true;
            }
            Instruction::Call(address) => {
                self.stack_ptr += 1;
                self.stack[usize::from(self.stack_ptr)] = self.counter;
                self.counter = address;
                self.skip_increment = true;
            }
            Instruction::SkipEqualsByte(reg, byte) => {
                if self.v[usize::from(reg)] == byte {
                    self.inc_pc();
                }
            }
            Instruction::SkipNotEqualsByte(reg, byte) => {
                if self.v[usize::from(reg)] != byte {
                    self.inc_pc();
                }
            }
            Instruction::SkipEqualsRegister(reg1, reg2) => {
                if self.v[usize::from(reg1)] == self.v[usize::from(reg2)] {
                    self.inc_pc();
                }
            }
            Instruction::LoadByte(reg, byte) => {
                self.v[usize::from(reg)] = byte;
            }
            Instruction::AddByte(reg, byte) => {
                self.v[usize::from(reg)] = self.add(self.v[usize::from(reg)], byte);
            }
            Instruction::LoadRegister(reg1, reg2) => {
                self.v[usize::from(reg1)] = self.v[usize::from(reg2)];
            }
            Instruction::Or(reg1, reg2) => {
                self.v[usize::from(reg1)] |= self.v[usize::from(reg2)];
            }
            Instruction::And(reg1, reg2) => {
                self.v[usize::from(reg1)] &= self.v[usize::from(reg2)];
            }
            Instruction::Xor(reg1, reg2) => {
                self.v[usize::from(reg1)] ^= self.v[usize::from(reg2)];
            }
            Instruction::AddRegister(reg1, reg2) => {
                self.v[usize::from(reg1)] =
                    self.add(self.v[usize::from(reg1)], self.v[usize::from(reg2)]);
            }
            Instruction::LoadImmediate(address) => {
                self.i = address;
            }
            Instruction::Random(register, data) => {
                let random_byte = rand::thread_rng().gen_range(0, 255);
                self.v[usize::from(register)] = random_byte & data;
            }
            Instruction::LoadFromDelay(register) => {
                self.v[usize::from(register)] = self.delay_register;
            }
            Instruction::LoadDelay(register) => {
                self.delay_register = register;
            }
            Instruction::LoadSound(register) => {
                self.sound_register = register;
            }
            Instruction::AddI(register) => {
                self.i = self.add_16(self.i, u16::from(self.v[usize::from(register)]));
            }
            Instruction::LoadIBCD(register) => {
                // Store BCD representation of Vx in memory locations I, I+1 and I+2.
                self.mem.mem[usize::from(self.i)] = register / 100;
                self.mem.mem[usize::from(self.i) + 1] = (register / 10) % 10;
                self.mem.mem[usize::from(self.i) + 2] = register % 10;
            }
            Instruction::StoreRegisters(register) => {
                let register: usize = usize::from(register);
                for n in 0..=register {
                    self.mem.mem[usize::from(self.i) + n] = self.v[n];
                }
                trace!("{:?}", self.mem);
            }
            Instruction::LoadRegisters(register) => {
                let register: usize = usize::from(register);
                for n in 0..=register {
                    self.v[n] = self.mem.mem[usize::from(self.i) + n]
                }
                debug!("{:?}", self.mem);
            }
            _ => unimplemented!(),
        };
        trace!("{:?}", self);
    }

    // Resets the machine back to the original state
    pub fn reset(&mut self) -> Result<(), String> {
        self.counter = 512;
        self.stack_ptr = 0;
        self.mem.mem = [0; MEMORY_SIZE];
        self.stack = [0; STACK_SIZE];
        self.v = [0; REGISTER_COUNT];
        self.i = 0;
        self.delay_register = 0;
        self.sound_register = 0;
        Ok(())
    }

    // Start the virtual machine: This is the fun part!
    pub fn start(&mut self) -> Result<(), String> {
        loop {
            // we check for 4095 because we need to read 2 bytes.
            if self.counter > 4095 {
                return Err(String::from("PC out of bounds"));
            }
            let opcode = {
                let pc: usize = usize::from(self.counter);
                Self::get_opcode(&self.mem.mem[pc..=pc + 1])
            };
            if opcode != 0 {
                trace!("PC: {}, opcode = {:X}", self.counter, opcode);
            }
            let instruction = self
                .instruction_parser
                .try_from(opcode)
                .expect("Could not parse opcode");
            trace!("Instruction: {:X?}", instruction);
            self.execute(&instruction);
            if !self.skip_increment {
                self.inc_pc();
            }
            self.skip_increment = false;
        }
    }
}

#[cfg(test)]
use std::io::{Seek, SeekFrom, Write};

mod tests {
    use super::*;
    use crate::opcodes::OpcodeMaskParser;
    use crate::opcodesv2::OpcodeTable;

    #[test]
    fn test_copy_into_mem_no_data() {
        let mut tmpfile = tempfile::tempfile().unwrap();
        let mut vm = Machine::new("TestVM", OpcodeTable {});
        vm._copy_into_mem(&mut tmpfile).unwrap();
        assert_eq!(vm.mem.mem.len(), 4096);
        // every byte in memory is zero when file is empty
        for byte in vm.mem.mem.iter() {
            assert_eq!(*byte, 0);
        }
    }

    #[test]
    fn test_copy_into_mem_some_data() {
        let mut tmpfile = tempfile::tempfile().unwrap();
        let mut vm = Machine::new("TestVM", OpcodeTable {});
        write!(tmpfile, "Hello World!").unwrap(); // Write
        tmpfile.seek(SeekFrom::Start(0)).unwrap(); // Seek to start
        vm._copy_into_mem(&mut tmpfile).unwrap();
        let expected = [72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100, 33];
        let mut count = 0;
        for _ in 0..expected.len() {
            assert_eq!(vm.mem.mem[PROGRAM_OFFSET + count], expected[count]);
            count += 1;
        }
    }

    #[test]
    fn test_create_opcode() {
        assert_eq!(Machine::<OpcodeTable>::get_opcode(&[0x31, 0x42]), 0x3142);
        assert_eq!(Machine::<OpcodeTable>::get_opcode(&[0x1, 0x2]), 0x0102);
        assert_eq!(Machine::<OpcodeTable>::get_opcode(&[0xAB, 0x9C]), 0xAB9C);

        // doesn't magically append or prepend zeroes to the final output
        assert_ne!(Machine::<OpcodeTable>::get_opcode(&[0x1, 0x2]), 0x1200);
        assert_ne!(Machine::<OpcodeTable>::get_opcode(&[0x1, 0x2]), 0x0012);
    }

    #[test]
    fn test_execute_cls() {
        // The way we test execute is to pass in an instruction and then
        // inspect the entire state of the machine for changes.
        // TODO: We might need a reset method to go back to the original state
        // Each instruction has a primary task and might also potentially have
        // some side-effect. We need to test both
        let mut machine = Machine::new("TestVM", OpcodeMaskParser {});
        machine.execute(&Instruction::ClearScreen);
        assert_eq!(machine.counter, 512);
        assert_eq!(machine.stack_ptr, 0);

        assert_eq!(machine.mem.mem.len(), 4096);
        // every byte in memory is zero when file is empty
        for byte in machine.mem.mem.iter() {
            assert_eq!(*byte, 0);
        }

        assert_eq!(machine.stack, [0; STACK_SIZE]);
        assert_eq!(machine.v, [0; REGISTER_COUNT]);
        assert_eq!(machine.i, 0);
        assert_eq!(machine.delay_register, 0);
        assert_eq!(machine.sound_register, 0);
    }

    #[test]
    fn test_execute_ret() {
        let mut machine = Machine::new("TestVM", OpcodeMaskParser {});
        // TODO: Should we artificially introduce modifications in the machine to test behaviour?
        // TODO: Perhaps a fixture-like ROM which is read before each test run.
        // Seems like it would be necessary otherwise a lot of behaviour can't be tested.
        // Modify the counter and the stack pointer before the machine execution starts
        machine.counter = 1;
        machine.stack_ptr = 1;
        machine.execute(&Instruction::Return);
        assert_eq!(machine.counter, 0);
        assert_eq!(machine.stack_ptr, 0);
        assert_eq!(machine.skip_increment, true);
        assert_eq!(machine.mem.mem.len(), 4096);
        // every byte in memory is zero when file is empty
        for byte in machine.mem.mem.iter() {
            assert_eq!(*byte, 0);
        }
        assert_eq!(machine.stack, [0; STACK_SIZE]);
        assert_eq!(machine.v, [0; REGISTER_COUNT]);
        assert_eq!(machine.i, 0);
        assert_eq!(machine.delay_register, 0);
        assert_eq!(machine.sound_register, 0);
    }

    #[test]
    fn test_execute_sys() {
        let mut machine = Machine::new("TestVM", OpcodeMaskParser {});
        machine.execute(&Instruction::SYS);
        assert_eq!(machine.counter, 512);
        assert_eq!(machine.stack_ptr, 0);
        assert_eq!(machine.skip_increment, false);
        assert_eq!(machine.mem.mem.len(), 4096);
        // every byte in memory is zero when file is empty
        for byte in machine.mem.mem.iter() {
            assert_eq!(*byte, 0);
        }
        assert_eq!(machine.stack, [0; STACK_SIZE]);
        assert_eq!(machine.v, [0; REGISTER_COUNT]);
        assert_eq!(machine.i, 0);
        assert_eq!(machine.delay_register, 0);
        assert_eq!(machine.sound_register, 0);
    }

    #[test]
    fn test_execute_jump() {
        let mut machine = Machine::new("TestVM", OpcodeMaskParser {});

        assert_eq!(machine.counter, 512); // before machine executes instruction

        machine.execute(&Instruction::Jump(0x0222));
        assert_eq!(machine.counter, 0x0222);

        machine.execute(&Instruction::Jump(4095));
        assert_eq!(machine.counter, 4095);

        assert_eq!(machine.stack_ptr, 0);
        assert_eq!(machine.skip_increment, true);
        assert_eq!(machine.mem.mem.len(), 4096);
        // every byte in memory is zero when file is empty
        for byte in machine.mem.mem.iter() {
            assert_eq!(*byte, 0);
        }
        assert_eq!(machine.stack, [0; STACK_SIZE]);
        assert_eq!(machine.v, [0; REGISTER_COUNT]);
        assert_eq!(machine.i, 0);
        assert_eq!(machine.delay_register, 0);
        assert_eq!(machine.sound_register, 0);
    }

    #[test]
    fn test_execute_call() {
        let mut machine = Machine::new("TestVM", OpcodeMaskParser {});

        assert_eq!(machine.counter, 512); // before machine executes instruction
        assert_eq!(machine.stack_ptr, 0);

        machine.counter = 25;
        machine.execute(&Instruction::Call(0x0222));
        assert_eq!(machine.stack_ptr, 1); // increments the stack pointer
        assert_eq!(machine.counter, 0x0222); // pushes the current pc to the stack
        assert_eq!(machine.skip_increment, true); // we're gonna skip the next automatic pc increment
        assert_eq!(machine.stack[usize::from(machine.stack_ptr)], 25); // stack has the old pc

        assert_eq!(machine.mem.mem.len(), 4096);
        // every byte in memory is zero when file is empty
        for byte in machine.mem.mem.iter() {
            assert_eq!(*byte, 0);
        }
        assert_eq!(machine.v, [0; REGISTER_COUNT]);
        assert_eq!(machine.i, 0);
        assert_eq!(machine.delay_register, 0);
        assert_eq!(machine.sound_register, 0);
    }

    #[test]
    fn test_execute_se() {
        let mut machine = Machine::new("TestVM", OpcodeMaskParser {});

        assert_eq!(machine.counter, 512); // before machine executes instruction
        machine.execute(&Instruction::SkipEqualsByte(machine.v[1], 0x0001)); // nothing should happen
        assert_eq!(machine.counter, 512);

        machine.v[1] = 0x0001;
        machine.execute(&Instruction::SkipEqualsByte(machine.v[1], 0x0001)); // nothing should happen
        assert_eq!(machine.counter, 514);

        assert_eq!(machine.mem.mem.len(), 4096);
        // every byte in memory is zero when file is empty
        for byte in machine.mem.mem.iter() {
            assert_eq!(*byte, 0);
        }
        assert_eq!(machine.i, 0);
        assert_eq!(machine.delay_register, 0);
        assert_eq!(machine.sound_register, 0);
    }

    #[test]
    fn test_execute_sne() {
        let mut machine = Machine::new("TestVM", OpcodeMaskParser {});

        assert_eq!(machine.counter, 512); // before machine executes instruction
        machine.v[1] = 0x0001;
        machine.execute(&Instruction::SkipNotEqualsByte(machine.v[1], 0x0001));
        assert_eq!(machine.counter, 512);

        machine.reset();
        machine.v[1] = 0x0001;

        machine.execute(&Instruction::SkipNotEqualsByte(machine.v[1], 0x0002));
        assert_eq!(machine.counter, 514);

        assert_eq!(machine.mem.mem.len(), 4096);
        // every byte in memory is zero when file is empty
        for byte in machine.mem.mem.iter() {
            assert_eq!(*byte, 0);
        }
        assert_eq!(machine.i, 0);
        assert_eq!(machine.delay_register, 0);
        assert_eq!(machine.sound_register, 0);
    }

    #[test]
    fn test_execute_se_reg() {
        let mut machine = Machine::new("TestVM", OpcodeMaskParser {});

        assert_eq!(machine.counter, 512); // before machine executes instruction
        machine.v[1] = 0x0001;
        machine.v[12] = 0x0001;
        machine.execute(&Instruction::SkipEqualsRegister(
            machine.v[1],
            machine.v[12],
        ));
        assert_eq!(machine.counter, 514);

        machine.v[1] = 0x0002;
        machine.execute(&Instruction::SkipEqualsRegister(
            machine.v[1],
            machine.v[12],
        ));
        assert_eq!(machine.counter, 514);

        assert_eq!(machine.mem.mem.len(), 4096);
        // every byte in memory is zero when file is empty
        for byte in machine.mem.mem.iter() {
            assert_eq!(*byte, 0);
        }
        assert_eq!(machine.i, 0);
        assert_eq!(machine.delay_register, 0);
        assert_eq!(machine.sound_register, 0);
    }
}
