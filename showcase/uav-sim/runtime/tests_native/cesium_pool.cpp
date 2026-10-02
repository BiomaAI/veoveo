// Runs against the patched upstream header before building the Cesium extension.
#include <cesium/omniverse/ObjectPool.h>

#include <iostream>
#include <stdexcept>
#include <vector>

namespace {
void require(bool condition, const char* message) {
    if (!condition) {
        throw std::runtime_error(message);
    }
}

struct Resource {
    uint64_t id;
    bool active{false};
};

class Pool final : public cesium::omniverse::ObjectPool<Resource> {
  public:
    mutable uint64_t created{0};

  protected:
    std::shared_ptr<Resource> createObject(uint64_t id) const override {
        ++created;
        return std::make_shared<Resource>(Resource{id});
    }

    void setActive(Resource* resource, bool active) const override {
        require(resource->active != active, "resource activation must alternate");
        resource->active = active;
    }
};
} // namespace

int main() {
    try {
        Pool pool;
        std::vector<std::shared_ptr<Resource>> resources;
        constexpr uint64_t count = 8192;
        for (uint64_t i = 0; i < count; ++i) {
            resources.push_back(pool.acquire());
            require(pool.created == i + 1, "acquisition prepared speculative GPU resources");
            require(pool.getNumberActive() == i + 1, "active count disagrees with ownership");
            require(resources.back()->id == i, "new resources must have distinct identities");
        }
        for (uint64_t i = 0; i < count; i += 2) {
            pool.release(resources[i]);
            require(!resources[i]->active, "released resources must be inactive");
            require(resources[i + 1]->active, "release changed an unrelated live resource");
        }
        for (uint64_t i = 0; i < count; i += 2) {
            auto reused = pool.acquire();
            require(reused == resources[i], "released resources must be reused");
        }
        require(pool.created == count, "cache reuse allocated more resources");
        for (const auto& resource : resources) {
            pool.release(resource);
        }
        require(pool.isEmpty(), "all resources should now be reusable");
        require(pool.getCapacity() == count, "release discarded reusable capacity");
        for (uint64_t i = 0; i < count; ++i) {
            require(pool.acquire() == resources[i], "full pool reuse changed resource identity");
        }
        require(pool.created == count, "full pool reuse allocated more resources");

        Pool reserved;
        reserved.setCapacity(32);
        for (uint64_t i = 0; i < 32; ++i) {
            reserved.acquire();
            require(reserved.created == 32, "75-percent occupancy must not trigger growth");
        }
        reserved.acquire();
        require(reserved.created == 33, "exhausted reserve should add one resource");
        std::cout << "{\"check\":\"cesium-demand-pools\",\"status\":\"passed\","
                     "\"maximumActiveResources\":8192}\n";
    } catch (const std::exception& error) {
        std::cerr << "Cesium pool qualification failed: " << error.what() << '\n';
        return 1;
    }
}
