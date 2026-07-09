import { useEffect, useMemo, useState } from 'react';
import {
  Background,
  BackgroundVariant,
  Handle,
  MarkerType,
  Position,
  ReactFlow,
  type Edge,
  type Node,
  type NodeProps,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import './WorkflowFlow.css';

type WorkflowNodeData = {
  title: string;
  kind: string;
  detail: string;
  tone: 'input' | 'function' | 'agent' | 'verify' | 'email';
  targetPosition?: Position;
  sourcePosition?: Position;
  hasSource?: boolean;
};

const steps = [
  'input',
  'stock-data',
  'summarize',
  'verify',
  'email',
] as const;

const nodeData: Array<Node<WorkflowNodeData>> = [
  {
    id: 'input',
    type: 'workflow',
    position: { x: 24, y: 116 },
    data: {
      title: 'User input',
      kind: 'Trigger',
      detail: 'Ticker symbols',
      tone: 'input',
    },
  },
  {
    id: 'stock-data',
    type: 'workflow',
    position: { x: 274, y: 42 },
    data: {
      title: 'Pull stock data',
      kind: 'Function Task',
      detail: 'Calls market data URLs',
      tone: 'function',
    },
  },
  {
    id: 'summarize',
    type: 'workflow',
    position: { x: 524, y: 42 },
    data: {
      title: 'Summarize data',
      kind: 'Agent Task',
      detail: 'Agent turns data into insight',
      tone: 'agent',
    },
  },
  {
    id: 'verify',
    type: 'workflow',
    position: { x: 524, y: 198 },
    data: {
      title: 'Verify summary',
      kind: 'Verifier Agent Task',
      detail: 'Another agent checks expected information',
      tone: 'verify',
      targetPosition: Position.Right,
      sourcePosition: Position.Left,
    },
  },
  {
    id: 'email',
    type: 'workflow',
    position: { x: 274, y: 198 },
    data: {
      title: 'Send email',
      kind: 'Function Task',
      detail: 'Delivers the verified report',
      tone: 'email',
      targetPosition: Position.Right,
      hasSource: false,
    },
  },
];

const edgeData: Edge[] = [
  {
    id: 'input-stock-data',
    source: 'input',
    target: 'stock-data',
    type: 'smoothstep',
    markerEnd: { type: MarkerType.ArrowClosed },
  },
  {
    id: 'stock-data-summarize',
    source: 'stock-data',
    target: 'summarize',
    type: 'smoothstep',
    markerEnd: { type: MarkerType.ArrowClosed },
  },
  {
    id: 'summarize-verify',
    source: 'summarize',
    sourceHandle: 'source-right',
    target: 'verify',
    targetHandle: 'target-right',
    type: 'smoothstep',
    markerEnd: { type: MarkerType.ArrowClosed },
  },
  {
    id: 'verify-email',
    source: 'verify',
    sourceHandle: 'source-left',
    target: 'email',
    targetHandle: 'target-right',
    type: 'smoothstep',
    markerEnd: { type: MarkerType.ArrowClosed },
  },
];

function WorkflowNode({ data, selected }: NodeProps<Node<WorkflowNodeData>>) {
  const targetPosition = data.targetPosition ?? Position.Left;
  const sourcePosition = data.sourcePosition ?? Position.Right;
  const hasSource = data.hasSource ?? true;

  return (
    <div className={`workflow-node workflow-node--${data.tone} ${selected ? 'is-active' : ''}`}>
      <Handle
        id={`target-${targetPosition.toLowerCase()}`}
        className="workflow-handle"
        type="target"
        position={targetPosition}
        isConnectable={false}
      />
      {hasSource ? (
        <Handle
          id={`source-${sourcePosition.toLowerCase()}`}
          className="workflow-handle"
          type="source"
          position={sourcePosition}
          isConnectable={false}
        />
      ) : null}
      <div className="workflow-node__meta">{data.kind}</div>
      <div className="workflow-node__title">{data.title}</div>
      <div className="workflow-node__detail">{data.detail}</div>
    </div>
  );
}

const nodeTypes = {
  workflow: WorkflowNode,
};

export default function WorkflowFlow() {
  const [activeStep, setActiveStep] = useState(0);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setActiveStep((current) => (current + 1) % steps.length);
    }, 1450);

    return () => window.clearInterval(timer);
  }, []);

  const activeNodeId = steps[activeStep];
  const activeEdgeId =
    activeStep === steps.length - 1
      ? undefined
      : `${steps[activeStep]}-${steps[activeStep + 1]}`;

  const nodes = useMemo(
    () =>
      nodeData.map((node) => ({
        ...node,
        draggable: false,
        selectable: false,
        selected: node.id === activeNodeId,
      })),
    [activeNodeId],
  );

  const edges = useMemo(
    () =>
      edgeData.map((edge) => ({
        ...edge,
        animated: edge.id === activeEdgeId,
        className: edge.id === activeEdgeId ? 'workflow-edge is-active' : 'workflow-edge',
        focusable: false,
        selectable: false,
      })),
    [activeEdgeId],
  );

  return (
    <div className="workflow-flow" aria-label="Stock report workflow data flow">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        nodesDraggable={false}
        nodesConnectable={false}
        nodesFocusable={false}
        edgesFocusable={false}
        elementsSelectable={false}
        panOnDrag={false}
        panOnScroll={false}
        zoomOnDoubleClick={false}
        zoomOnPinch={false}
        zoomOnScroll={false}
        preventScrolling={false}
        fitView
        fitViewOptions={{ padding: 0.12 }}
        minZoom={0.5}
        maxZoom={1.1}
        proOptions={{ hideAttribution: true }}
      >
        <Background color="#d8e1eb" gap={22} size={1} variant={BackgroundVariant.Dots} />
      </ReactFlow>
    </div>
  );
}
