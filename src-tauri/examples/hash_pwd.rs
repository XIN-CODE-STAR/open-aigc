// 一次性工具：把 `123` 的 argon2id 哈希打印到 stdout。
// 用法：cargo run --example hash_pwd_123
// 仅用于本地重置测试管理员密码。
use argon2::password_hash::{PasswordHasher, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::rngs::OsRng;

fn main() {
    let password = std::env::args().nth(1).unwrap_or_else(|| "123".to_string());
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default());
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .expect("hash password")
        .to_string();
    println!("{hash}");
}
