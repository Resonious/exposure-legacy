# frozen_string_literal: true

module Exposure
  # Light wrapper around the Core Trace
  class Trace
    def initialize(path_whitelist: //)
      @core = Core.new_trace

      @trace_points = []
      @trace_points << TracePoint.new(:class, :call, &method(:push))
      @trace_points << TracePoint.new(:return, :end, &method(:pop))

      @class_name = Class.method(:to_s).unbind
      @module_name = Module.method(:to_s).unbind
      @is_a = Object.instance_method(:is_a?)

      @path_whitelist = path_whitelist
    end

    def name_of(klass)
      case klass
      when Class then @class_name.bind(klass).call
      when Module then @module_name.bind(klass).call
      end
    end

    def start
      @trace_points.each(&:enable)
    end

    def stop
      @trace_points.each(&:disable)
    end

    def push(trace)
      # puts "#{ident_up} push #{trace.path}"
      return unless trace.path =~ @path_whitelist

      calla = caller_locations(2..2).first
      if @is_a.bind(trace.binding.receiver).call(Class)
        receiver = trace.binding.receiver
      end

      klass = trace.defined_class
      klass = klass.instance_method(trace.method_id).owner if trace.method_id

      # First push
      Core.push_frame(
        @core,

        trace.event,

        calla.path,
        calla.lineno,

        trace.path,
        trace.lineno,

        name_of(klass),
        trace.method_id.to_s,

        name_of(receiver)
      )

      # Then add locals
      add_locals(trace.binding)
    end

    def pop(trace)
      # puts "#{ident_down} pop #{trace.path}"
      return unless trace.path =~ @path_whitelist

      if trace.event == :return || trace.event == :b_return
        return_class = trace.return_value.class
        return_type = name_of(return_class)
      end

      add_locals(trace.binding)
      Core.pop_frame(@core, return_type)
    end

    private

    def ident
      @ident ||= 0
      ' ' * @ident
    end

    def ident_up
      @ident ||= 0
      @ident += 1
      ident
    end

    def ident_down
      @ident ||= 0
      result = ident
      @ident -= 1
      raise 'IDENT WENT NEGATIVE' if @ident.negative?

      result
    end

    def add_locals(frame_binding)
      frame_binding.local_variables.each do |var|
        begin
          val = frame_binding.local_variable_get(var)
          Core.add_local(@core, var.to_s, name_of(val.class))
        rescue StandardError => e
          Core.add_local(@core, var.to_s, "((#{e.class} during inspect))")
        end
      end
    end
  end
end
