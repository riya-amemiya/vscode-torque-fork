// Copyright 2026 Riya Amemiya.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

export type Position = {
  line: number;
  character: number;
};

export type Range = {
  start: Position;
  end: Position;
};

export type LineTable = {
  text: string;
  starts: number[];
};

export function createLineTable(text: string): LineTable {
  const starts = [0];
  for (let index = 0; index < text.length; index += 1) {
    if (text.charCodeAt(index) === 10) {
      starts.push(index + 1);
    }
  }
  return { text, starts };
}

export function offsetToPosition(table: LineTable, offset: number): Position {
  const clamped = Math.max(0, Math.min(offset, table.text.length));
  let low = 0;
  let high = table.starts.length - 1;
  while (low < high) {
    const mid = Math.ceil((low + high + 1) / 2);
    if (table.starts[mid] <= clamped) {
      low = mid;
    } else {
      high = mid - 1;
    }
  }
  return { line: low, character: clamped - table.starts[low] };
}

export function rangeFromOffsets(table: LineTable, start: number, end: number): Range {
  return {
    start: offsetToPosition(table, start),
    end: offsetToPosition(table, end),
  };
}

export function positionToOffset(table: LineTable, position: Position): number {
  const lineStart = table.starts[Math.min(position.line, table.starts.length - 1)] ?? 0;
  const nextLineStart =
    position.line + 1 < table.starts.length ? table.starts[position.line + 1] : table.text.length;
  return Math.min(lineStart + Math.max(position.character, 0), nextLineStart);
}
