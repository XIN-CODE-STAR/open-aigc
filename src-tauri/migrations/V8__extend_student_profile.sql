--- V8: 扩展学生信息字段（性别、手机号、身份证号码）用于批量导入
-- gender: 学生性别（male/female/unknown），默认 unknown
ALTER TABLE student ADD COLUMN gender TEXT NOT NULL DEFAULT 'unknown'
  CHECK (gender IN ('male', 'female', 'unknown'));

-- phone: 学生手机号，可空
ALTER TABLE student ADD COLUMN phone TEXT;

-- id_card_number: 身份证号码，可空
ALTER TABLE student ADD COLUMN id_card_number TEXT;
