// g15stress: exercise G15 VM slots and the compute stamp wrap (omacom/linux-aurora#233).
//
//   g15stress vms PROCS ITERS [BATCH]   PROCS processes each open a Vulkan device (a GPU VM),
//                                       all wait until every one is open, then each runs ITERS
//                                       verified submits of BATCH compute command buffers.
//   g15stress jobs TOTAL [BATCH]        one process runs TOTAL compute command buffers, BATCH per
//                                       submit, verifying each submit; prints the rate.
//
// Each command buffer is one dispatch that bumps an atomic counter once; a submit passes when
// the counter equals BATCH and the data words hold the expected pattern.
#include <vulkan/vulkan.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>
#include <unistd.h>
#include <sys/wait.h>
#include <poll.h>

#define WORDS 1024
#define CHECK(x) do { VkResult r_ = (x); if (r_ != VK_SUCCESS) { \
	fprintf(stderr, "[%d] %s failed: %d\n", getpid(), #x, r_); exit(10); } } while (0)

struct ctx {
	VkInstance inst; VkPhysicalDevice pd; VkDevice dev; VkQueue q;
	VkBuffer buf; VkDeviceMemory mem; uint32_t *map;
	VkPipeline pipe; VkPipelineLayout pl; VkDescriptorSet ds;
	VkCommandPool pool; VkCommandBuffer *cbs; VkFence fence; uint32_t batch;
};

static double now(void)
{
	struct timespec t;
	clock_gettime(CLOCK_MONOTONIC, &t);
	return t.tv_sec + t.tv_nsec / 1e9;
}

static uint32_t *load_spv(const char *path, size_t *size)
{
	FILE *f = fopen(path, "rb");
	if (!f) { perror(path); exit(2); }
	fseek(f, 0, SEEK_END); *size = ftell(f); fseek(f, 0, SEEK_SET);
	uint32_t *code = malloc(*size);
	if (fread(code, 1, *size, f) != *size) exit(2);
	fclose(f);
	return code;
}

static void init(struct ctx *c, const char *spv, uint32_t batch)
{
	c->batch = batch;
	VkApplicationInfo app = { .sType = VK_STRUCTURE_TYPE_APPLICATION_INFO, .apiVersion = VK_API_VERSION_1_1 };
	VkInstanceCreateInfo ici = { .sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO, .pApplicationInfo = &app };
	CHECK(vkCreateInstance(&ici, NULL, &c->inst));
	uint32_t n = 8; VkPhysicalDevice pds[8];
	CHECK(vkEnumeratePhysicalDevices(c->inst, &n, pds));
	c->pd = VK_NULL_HANDLE;
	for (uint32_t i = 0; i < n; i++) {
		VkPhysicalDeviceProperties p; vkGetPhysicalDeviceProperties(pds[i], &p);
		if (strstr(p.deviceName, "Apple")) { c->pd = pds[i]; break; }
	}
	if (!c->pd) { fprintf(stderr, "no Apple GPU device\n"); exit(3); }

	float prio = 1.0f;
	VkDeviceQueueCreateInfo qci = { .sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
		.queueFamilyIndex = 0, .queueCount = 1, .pQueuePriorities = &prio };
	VkDeviceCreateInfo dci = { .sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
		.queueCreateInfoCount = 1, .pQueueCreateInfos = &qci };
	CHECK(vkCreateDevice(c->pd, &dci, NULL, &c->dev));
	vkGetDeviceQueue(c->dev, 0, 0, &c->q);

	VkBufferCreateInfo bci = { .sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
		.size = (WORDS + 1) * 4, .usage = VK_BUFFER_USAGE_STORAGE_BUFFER_BIT };
	CHECK(vkCreateBuffer(c->dev, &bci, NULL, &c->buf));
	VkMemoryRequirements mr; vkGetBufferMemoryRequirements(c->dev, c->buf, &mr);
	VkPhysicalDeviceMemoryProperties mp; vkGetPhysicalDeviceMemoryProperties(c->pd, &mp);
	uint32_t want = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT, ti = ~0u;
	for (uint32_t i = 0; i < mp.memoryTypeCount; i++)
		if ((mr.memoryTypeBits & (1u << i)) && (mp.memoryTypes[i].propertyFlags & want) == want) { ti = i; break; }
	VkMemoryAllocateInfo mai = { .sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
		.allocationSize = mr.size, .memoryTypeIndex = ti };
	CHECK(vkAllocateMemory(c->dev, &mai, NULL, &c->mem));
	CHECK(vkBindBufferMemory(c->dev, c->buf, c->mem, 0));
	CHECK(vkMapMemory(c->dev, c->mem, 0, VK_WHOLE_SIZE, 0, (void **)&c->map));

	size_t sz; uint32_t *code = load_spv(spv, &sz);
	VkShaderModuleCreateInfo smci = { .sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO, .codeSize = sz, .pCode = code };
	VkShaderModule sm; CHECK(vkCreateShaderModule(c->dev, &smci, NULL, &sm));
	VkDescriptorSetLayoutBinding b = { 0, VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, 1, VK_SHADER_STAGE_COMPUTE_BIT, NULL };
	VkDescriptorSetLayoutCreateInfo dlci = { .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO, .bindingCount = 1, .pBindings = &b };
	VkDescriptorSetLayout dsl; CHECK(vkCreateDescriptorSetLayout(c->dev, &dlci, NULL, &dsl));
	VkPushConstantRange pcr = { VK_SHADER_STAGE_COMPUTE_BIT, 0, 4 };
	VkPipelineLayoutCreateInfo plci = { .sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
		.setLayoutCount = 1, .pSetLayouts = &dsl, .pushConstantRangeCount = 1, .pPushConstantRanges = &pcr };
	CHECK(vkCreatePipelineLayout(c->dev, &plci, NULL, &c->pl));
	VkComputePipelineCreateInfo cpci = { .sType = VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
		.stage = { .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO, .stage = VK_SHADER_STAGE_COMPUTE_BIT, .module = sm, .pName = "main" },
		.layout = c->pl };
	CHECK(vkCreateComputePipelines(c->dev, VK_NULL_HANDLE, 1, &cpci, NULL, &c->pipe));

	VkDescriptorPoolSize ps = { VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, 1 };
	VkDescriptorPoolCreateInfo dpci = { .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO, .maxSets = 1, .poolSizeCount = 1, .pPoolSizes = &ps };
	VkDescriptorPool dp; CHECK(vkCreateDescriptorPool(c->dev, &dpci, NULL, &dp));
	VkDescriptorSetAllocateInfo dsai = { .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO, .descriptorPool = dp, .descriptorSetCount = 1, .pSetLayouts = &dsl };
	CHECK(vkAllocateDescriptorSets(c->dev, &dsai, &c->ds));
	VkDescriptorBufferInfo dbi = { c->buf, 0, VK_WHOLE_SIZE };
	VkWriteDescriptorSet w = { .sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET, .dstSet = c->ds, .descriptorCount = 1,
		.descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, .pBufferInfo = &dbi };
	vkUpdateDescriptorSets(c->dev, 1, &w, 0, NULL);

	VkCommandPoolCreateInfo cpi = { .sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO, .queueFamilyIndex = 0 };
	CHECK(vkCreateCommandPool(c->dev, &cpi, NULL, &c->pool));
	c->cbs = calloc(batch, sizeof(*c->cbs));
	VkCommandBufferAllocateInfo cbai = { .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
		.commandPool = c->pool, .level = VK_COMMAND_BUFFER_LEVEL_PRIMARY, .commandBufferCount = batch };
	CHECK(vkAllocateCommandBuffers(c->dev, &cbai, c->cbs));
	for (uint32_t i = 0; i < batch; i++) {
		VkCommandBufferBeginInfo bi = { .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO };
		CHECK(vkBeginCommandBuffer(c->cbs[i], &bi));
		vkCmdBindPipeline(c->cbs[i], VK_PIPELINE_BIND_POINT_COMPUTE, c->pipe);
		vkCmdBindDescriptorSets(c->cbs[i], VK_PIPELINE_BIND_POINT_COMPUTE, c->pl, 0, 1, &c->ds, 0, NULL);
		uint32_t seed = 0x1000 + i;
		vkCmdPushConstants(c->cbs[i], c->pl, VK_SHADER_STAGE_COMPUTE_BIT, 0, 4, &seed);
		vkCmdDispatch(c->cbs[i], WORDS / 64, 1, 1);
		CHECK(vkEndCommandBuffer(c->cbs[i]));
	}
	VkFenceCreateInfo fci = { .sType = VK_STRUCTURE_TYPE_FENCE_CREATE_INFO };
	CHECK(vkCreateFence(c->dev, &fci, NULL, &c->fence));
}

// One verified submit; returns 0 on pass.
static int run_once(struct ctx *c)
{
	memset(c->map, 0, (WORDS + 1) * 4);
	VkSubmitInfo si = { .sType = VK_STRUCTURE_TYPE_SUBMIT_INFO, .commandBufferCount = c->batch, .pCommandBuffers = c->cbs };
	CHECK(vkQueueSubmit(c->q, 1, &si, c->fence));
	VkResult r = vkWaitForFences(c->dev, 1, &c->fence, VK_TRUE, 5000000000ull);
	if (r != VK_SUCCESS) { fprintf(stderr, "[%d] fence wait: %d\n", getpid(), r); return 1; }
	vkResetFences(c->dev, 1, &c->fence);
	if (c->map[WORDS] != c->batch) {
		fprintf(stderr, "[%d] counter %u, want %u\n", getpid(), c->map[WORDS], c->batch);
		return 1;
	}
	for (uint32_t i = 0; i < WORDS; i++) {
		uint32_t s = c->map[i] - i * 2654435761u;
		if (s < 0x1000 || s >= 0x1000 + c->batch) {
			fprintf(stderr, "[%d] word %u = 0x%x\n", getpid(), i, c->map[i]);
			return 1;
		}
	}
	return 0;
}

int main(int argc, char **argv)
{
	const char *spv = getenv("G15STRESS_SPV") ? getenv("G15STRESS_SPV") : "g15stress.spv";
	if (argc >= 4 && !strcmp(argv[1], "vms")) {
		int procs = atoi(argv[2]), iters = atoi(argv[3]);
		uint32_t batch = argc > 4 ? atoi(argv[4]) : 4;
		int ready[2], go[2];
		if (pipe(ready) || pipe(go)) return 2;
		for (int p = 0; p < procs; p++) {
			if (fork() == 0) {
				close(ready[0]); close(go[1]);
				struct ctx c; init(&c, spv, batch);
				char b = 'r'; if (write(ready[1], &b, 1) != 1) exit(4);
				if (read(go[0], &b, 1) < 0) exit(4);  // EOF when the parent releases everyone
				for (int i = 0; i < iters; i++)
					if (run_once(&c)) exit(1);
				exit(0);
			}
		}
		close(ready[1]); close(go[0]);
		// A child that fails to open its device exits without a ready byte: stop waiting
		// once no device has opened for 10 s.
		int opened = 0; char b;
		double t0 = now();
		struct pollfd pfd = { .fd = ready[0], .events = POLLIN };
		while (opened < procs && poll(&pfd, 1, 10000) > 0 && read(ready[0], &b, 1) == 1)
			opened++;
		printf("vms: %d/%d devices open at once (%.2f s)\n", opened, procs, now() - t0);
		fflush(stdout);
		close(go[1]);
		int pass = 0, fail = 0, initfail = 0, st;
		while (wait(&st) > 0) {
			if (WIFEXITED(st) && WEXITSTATUS(st) == 0) pass++;
			else if (WIFEXITED(st) && WEXITSTATUS(st) >= 10) initfail++;
			else fail++;
		}
		printf("vms: %d pass, %d run failures, %d init failures, %.2f s\n", pass, fail, initfail, now() - t0);
		return pass == procs ? 0 : 1;
	}
	if (argc >= 3 && !strcmp(argv[1], "jobs")) {
		long total = atol(argv[2]);
		uint32_t batch = argc > 3 ? atoi(argv[3]) : 64;
		struct ctx c; init(&c, spv, batch);
		double t0 = now(), last = t0;
		long done = 0;
		while (done < total) {
			if (run_once(&c)) { printf("jobs: FAIL after %ld command buffers\n", done); return 1; }
			done += batch;
			if (now() - last >= 10) {
				last = now();
				printf("jobs: %ld done, %.0f/s\n", done, done / (last - t0));
				fflush(stdout);
			}
		}
		printf("jobs: PASS %ld command buffers in %.1f s (%.0f/s)\n", done, now() - t0, done / (now() - t0));
		return 0;
	}
	fprintf(stderr, "usage: g15stress vms PROCS ITERS [BATCH] | jobs TOTAL [BATCH]\n");
	return 2;
}
