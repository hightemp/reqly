// The tag editor modal is rendered once by the shell; editors open it here.

export interface TagEditRequest {
  text: string;
  nodeId: string | null;
  apply: (next: string) => void;
}

class TagEditorState {
  request = $state<TagEditRequest | null>(null);

  open(request: TagEditRequest) {
    this.request = request;
  }

  close() {
    this.request = null;
  }
}

export const tagEditor = new TagEditorState();
