import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { createInterface } from "node:readline";
import { EventEmitter } from "node:events";

interface Reply {
  id?: number;
  result?: unknown;
  error?: unknown;
  event?: string;
  payload?: unknown;
}
export class Backend extends EventEmitter {
  private child: ChildProcessWithoutNullStreams;
  private next = 0;
  private pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (error: unknown) => void }
  >();
  private closed = false;
  constructor(executable: string, compatible: boolean) {
    super();
    this.child = spawn(
      executable,
      ["--ipc", ...(compatible ? ["--recovery-compatible"] : [])],
      { stdio: "pipe" },
    );
    this.child.stderr.on("data", (chunk: Buffer) =>
      process.stderr.write(chunk),
    );
    createInterface({ input: this.child.stdout }).on("line", (line) => {
      let reply: Reply;
      try {
        reply = JSON.parse(line) as Reply;
      } catch (error) {
        this.fail(error);
        return;
      }
      if (reply.event) {
        this.emit("event", reply.event, reply.payload);
        return;
      }
      const request =
        reply.id === undefined ? undefined : this.pending.get(reply.id);
      if (!request) return;
      this.pending.delete(reply.id!);
      if (reply.error !== undefined) request.reject(reply.error);
      else request.resolve(reply.result);
    });
    this.child.on("error", (error) => this.fail(error));
    this.child.on("exit", (code, signal) => {
      this.fail(new Error(`后台服务已退出 (${code ?? signal})`));
      this.emit("exit", code);
    });
    this.child.stdin.on("error", (error) => this.fail(error));
  }
  call(command: string, args?: unknown): Promise<unknown> {
    if (this.closed) return Promise.reject(new Error("后台服务已退出"));
    const id = ++this.next;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.child.stdin.write(
        JSON.stringify({ id, command, args: args ?? {} }) + "\n",
      );
    });
  }
  close() {
    this.child.stdin.end();
  }
  private fail(error: unknown) {
    this.closed = true;
    for (const request of this.pending.values()) request.reject(error);
    this.pending.clear();
  }
}
