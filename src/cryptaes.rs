use std::fmt;
use std::error::Error;

use crypto::{ buffer, aes, blockmodes };
use crypto::symmetriccipher::SymmetricCipherError;
use crypto::buffer::{ ReadBuffer, WriteBuffer, BufferResult };

#[derive(Debug)]
pub struct DecryptError(SymmetricCipherError);

impl Error for DecryptError {
    fn description(&self) -> &'static str {
        match self.0 {
            SymmetricCipherError::InvalidLength => "Decrypt error: invalid length",
            SymmetricCipherError::InvalidPadding => "Decrypt error: invalid padding"
        }
    }
}

impl fmt::Display for DecryptError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl From<SymmetricCipherError> for DecryptError {
    fn from(err: SymmetricCipherError) -> DecryptError {
        DecryptError(err)
    }
}

pub fn decrypt256(encrypted_data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, DecryptError> {
    let mut decryptor = aes::cbc_decryptor(
            aes::KeySize::KeySize256,
            key,
            iv,
            blockmodes::PkcsPadding);

    let mut final_result = Vec::<u8>::new();
    let mut read_buffer = buffer::RefReadBuffer::new(encrypted_data);
    let mut buffer = [0; 4096];
    let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);

    loop {
        let result = try!(decryptor.decrypt(&mut read_buffer, &mut write_buffer, true));
        final_result.extend(write_buffer.take_read_buffer().take_remaining().iter().map(|&i| i));
        match result {
            BufferResult::BufferUnderflow => break,
            BufferResult::BufferOverflow => { }
        }
    }

    Ok(final_result)
}
