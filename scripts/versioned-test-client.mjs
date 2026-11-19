// Isolated integration-test clients carry versions returned by their own API calls.
// Precondition-rejection tests deliberately use raw fetch instead of this adapter.
export function versionedTestClient() {
  const versions=new Map()
  return {
    prepare(path,method,body) {
      const parts=path.split('/')
      if(method==='PUT'&&parts[0]==='workflows'&&parts.length===2&&versions.has(parts[1])&&body?.version===undefined) return {...body,version:versions.get(parts[1])}
      if(method==='POST'&&parts[0]==='workflows'&&['publish','pause','resume','archive','restore'].includes(parts[2])&&versions.has(parts[1])&&body?.expectedVersion===undefined) return {...body,expectedVersion:versions.get(parts[1])}
      if(method==='POST'&&path==='runs'&&versions.has(body?.workflowId)&&body?.expectedVersion===undefined) return {...body,expectedVersion:versions.get(body.workflowId)}
      return body
    },
    remember(value) {
      for(const workflow of value.workflows||[]) if(Number.isInteger(workflow.version))versions.set(workflow.id,workflow.version)
      if(value.id&&Number.isInteger(value.version))versions.set(value.id,value.version)
      return value
    }
  }
}
