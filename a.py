import os
import re
import subprocess
import fnmatch
import queue

class Node:
	def __init__(self):
		self.to = []
		self.invTo = []
		self.cnt = 0

	def add_edge(self, to):
		self.to.append(to)
		self.cnt += 1

	def add_inv_edge(self, to):
		self.invTo.append(to)

def get_make_target(dir_name: str):
	file = open(os.path.join(dir_name, 'Make/files'))
	for l in file.readlines():
		l = l.replace('\n', '')
		l = l.replace(' ', '')
		l = l.split('=')
		if l[0] == 'EXE':
			file_path = l[1]
			exe_name = file_path.replace('$(FOAM_APPBIN)/', '')
			return l[0], exe_name
		elif l[0] == 'LIB':
			file_path = l[1]
			lib_name = file_path.replace('$(FOAM_LIBBIN)/', '')
			return l[0], lib_name

def get_dependencies(dir_name: str):
	option = open(os.path.join(dir_name, 'Make/options'))
	string = option.read()
	string = re.sub(r'\\.|\\\s', '', string)
	lines = string.split('\n')
	for l in lines:
		if l == '':
			continue
		dependency_type, files = l.split('=')
		dependency_type = dependency_type.replace(' ', '')
		if dependency_type == 'EXE_LIBS':
			files = files.split()
			for i in range(len(files)):
				files[i] = files[i][2:]
			return files
		if dependency_type == 'LIB_LIBS':
			files = files.split()
			for i in range(len(files)):
				files[i] = files[i][2:]
			return files

def find(path, name):
	for root, dirs, files in os.walk(path):
		for f in files:
			fn = os.path.join(root, f)
			if fnmatch.fnmatch(fn, name):
				yield fn

memo = {}
graph = {}

def solve(path):
	dependencies = get_dependencies(path)
	if dependencies == None:
		return dependencies
	wm_project_dir = os.environ['WM_PROJECT_DIR']
	files = find(wm_project_dir, '*Make/files')
	for dependency in dependencies:
		files = find(wm_project_dir, '*Make/files')
		for f in files:
			f = f[:-11]
			typ, name = get_make_target(f)
			if typ == 'LIB' and name == 'lib'+dependency:
				#path -> f
				if not f in graph:
					graph[f] = Node()
				graph[path].add_edge(f)
				graph[f].add_inv_edge(path)
				if not f in memo:
					memo[f] = solve(f)
	return dependencies

dfs_result = set()

def DFS(node):
	if len(graph[node].to) == 0:
		dfs_result.add(node)
	for to in graph[node].to:
		DFS(to)

graph['.'] = Node()
solve('.')
DFS('.')
que = queue.Queue()
for x in dfs_result:
	que.put(x)

while not que.empty():
	node = que.get()
	print(node)
	subprocess.call(["wmake", node])
	for to in graph[node].invTo:
		graph[to].cnt -= 1
		if graph[to].cnt == 0:
			que.put(to)