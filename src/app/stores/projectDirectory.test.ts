import { describe, expect, it } from "vitest";
import { projectDirName } from "./projectDirectory";

describe("projectDirName", () => {
  it("取 Windows 路径的最子级文件夹名", () => {
    expect(projectDirName("D:\\xin18\\Documents\\crab\\crabdemo")).toBe("crabdemo");
  });

  it("取 POSIX 路径的最子级文件夹名", () => {
    expect(projectDirName("/home/user/projects/demo")).toBe("demo");
  });

  it("容忍结尾斜杠", () => {
    expect(projectDirName("D:\\work\\demo\\")).toBe("demo");
    expect(projectDirName("D:\\work\\demo\\\\")).toBe("demo");
  });

  it("无分隔符时原样返回", () => {
    expect(projectDirName("demo")).toBe("demo");
  });
});
