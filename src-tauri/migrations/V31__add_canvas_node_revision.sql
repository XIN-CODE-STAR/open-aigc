-- V31: Add revision column to canvas_nodes for optimistic concurrency control.
-- Constitution §4: Agent is responsible for judgment, CanvasCommand for modification, CanvasRuntime for execution.
-- Revision ensures safe concurrent operations from multiple Agents.

ALTER TABLE canvas_nodes ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;
